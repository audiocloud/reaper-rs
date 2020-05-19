use std::cell::{Cell, Ref, RefCell, RefMut};
use std::collections::hash_map::Entry;
use std::collections::HashMap;

use std::ffi::{CStr, CString};

use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::{Arc, Weak};

use rxrust::prelude::*;

use crate::fx::Fx;
use crate::fx_parameter::FxParameter;
use crate::helper_control_surface::HelperControlSurface;
use crate::track_send::TrackSend;
use crate::undo_block::UndoBlock;
use crate::ActionKind::Toggleable;
use crate::{
    create_default_console_msg_formatter, create_reaper_panic_hook, create_std_logger,
    create_terminal_logger, Action, Guid, MidiInputDevice, MidiOutputDevice, Project, Reaper,
    Track,
};
use helgoboss_midi::{RawShortMessage, ShortMessage, ShortMessageType};
use once_cell::sync::Lazy;
use reaper_low::raw;

use reaper_low::ReaperPluginContext;

use crossbeam_channel::{Receiver, Sender};
use reaper_medium::ProjectContext::Proj;
use reaper_medium::UndoScope::All;
use reaper_medium::{
    CommandId, GetFocusedFxResult, GetLastTouchedFxResult, GlobalAutomationModeOverride, Hwnd,
    MediumGaccelRegister, MediumHookCommand, MediumHookPostCommand, MediumOnAudioBuffer,
    MediumToggleAction, MidiInputDeviceId, MidiOutputDeviceId, OnAudioBufferArgs, ProjectRef,
    RealTimeAudioThreadScope, ReaperStringArg, ReaperVersion, StuffMidiMessageTarget,
    ToggleActionResult, TrackRef,
};
use std::fmt::{Debug, Formatter};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

/// Capacity of the channel which is used to scheduled tasks for execution in the main thread.
///
/// Should probably be a bit more than MAX_AUDIO_THREAD_TASKS because the audio callback is
/// usually executed more often and therefore can produce faster. Plus, the main thread also
/// uses this very often to schedule tasks for a later execution in the main thread.
///
/// Shouldn't be too high because when `ReaperSession::deactivate()` is called, those tasks are
/// going to pile up - and they will be discarded on the next activate.
const MAX_MAIN_THREAD_TASKS: usize = 1000;

/// Capacity of the channel which is used to scheduled tasks for execution in the real-time audio
/// thread.
const MAX_AUDIO_THREAD_TASKS: usize = 500;

/// We  make sure in each public function/method that it's called from the correct thread. Similar
/// with other methods. We basically make this struct thread-safe by panicking whenever we are in
/// the wrong thread.
///
/// We could also go the easy way of using one Reaper instance wrapped in a Mutex. Downside: This is
/// more guarantees than we need. Why should audio thread and main thread fight for access to one
/// Reaper instance. That results in performance loss.
//
// This is safe (see https://doc.rust-lang.org/std/sync/struct.Once.html#examples-1).
static mut INSTANCE: Option<ReaperSession> = None;
static INIT_INSTANCE: std::sync::Once = std::sync::Once::new();

// Here we don't mind having a heavy mutex because this is not often accessed.
static REAPER_GUARD: Lazy<Mutex<Weak<ReaperGuard>>> = Lazy::new(|| Mutex::new(Weak::new()));

pub struct ReaperBuilder {
    medium: reaper_medium::ReaperSession,
    logger: Option<slog::Logger>,
}

impl ReaperBuilder {
    fn new(context: ReaperPluginContext) -> ReaperBuilder {
        ReaperBuilder {
            medium: {
                let low = reaper_low::Reaper::load(context);
                reaper_medium::ReaperSession::new(low)
            },
            logger: Default::default(),
        }
    }

    pub fn logger(mut self, logger: slog::Logger) -> ReaperBuilder {
        self.require_main_thread();
        self.logger = Some(logger);
        self
    }

    /// This has an effect only if there isn't an instance already.
    pub fn setup(self) {
        self.require_main_thread();
        unsafe {
            INIT_INSTANCE.call_once(|| {
                let logger = self.logger.unwrap_or_else(create_std_logger);
                // TODO Actually, one static variable carrying ReaperSession and Reaper would be
                //  enough here.
                Reaper::make_available_globally(Reaper::new(self.medium.reaper().clone()));
                let session = ReaperSession {
                    medium: RefCell::new(self.medium),
                    logger,
                    command_by_id: RefCell::new(HashMap::new()),
                    subjects: MainSubjects::new(),
                    undo_block_is_active: Cell::new(false),
                    main_thread_task_channel: crossbeam_channel::bounded::<MainThreadTask>(
                        MAX_MAIN_THREAD_TASKS,
                    ),
                    audio_thread_task_channel: crossbeam_channel::bounded::<AudioThreadTaskOp>(
                        MAX_AUDIO_THREAD_TASKS,
                    ),
                    active_data: RefCell::new(None),
                };
                INSTANCE = Some(session)
            });
        }
    }

    fn require_main_thread(&self) {
        require_main_thread(self.medium.reaper().low().plugin_context());
    }
}

// TODO Maybe introduce also a RealTimeReaper and hold this instead of medium.
//  Of course this is obsolete if we ditch the difference between ReaperSession and Reaper because
//  it doesn't make so much sense in high-level REAPER.
pub struct RealTimeReaperSession {
    medium: reaper_medium::Reaper<RealTimeAudioThreadScope>,
    midi_message_received: LocalSubject<'static, MidiEvent<RawShortMessage>, ()>,
    main_thread_task_sender: Sender<MainThreadTask>,
}

impl RealTimeReaperSession {
    pub fn midi_message_received(
        &self,
    ) -> impl LocalObservable<'static, Err = (), Item = MidiEvent<RawShortMessage>> {
        self.midi_message_received.clone()
    }
}

struct HighOnAudioBuffer {
    audio_thread_task_receiver: Receiver<AudioThreadTaskOp>,
    session: RealTimeReaperSession,
}

impl MediumOnAudioBuffer for HighOnAudioBuffer {
    fn call(&mut self, args: OnAudioBufferArgs) {
        if args.is_post {
            return;
        }
        // Take only one task each time because we don't want to do to much in one go in the
        // real-time thread.
        for task in self.audio_thread_task_receiver.try_iter().take(1) {
            (task)(&self.session);
        }
        // Process MIDI
        let subject = &mut self.session.midi_message_received;
        if subject.subscribed_size() == 0 {
            return;
        }
        for i in 0..self.session.medium.get_max_midi_inputs() {
            self.session
                .medium
                .get_midi_input(MidiInputDeviceId::new(i as u8), |input| {
                    let evt_list = input.get_read_buf();
                    for evt in evt_list.enum_items(0) {
                        let msg = evt.message();
                        if msg.r#type() == ShortMessageType::ActiveSensing {
                            // TODO-low We should forward active sensing. Can be filtered out
                            // later.
                            continue;
                        }
                        let owned_msg: RawShortMessage = msg.to_other();
                        let owned_evt = MidiEvent::new(evt.frame_offset(), owned_msg);
                        subject.next(owned_evt);
                    }
                });
        }
    }
}

#[derive(Debug)]
pub struct ReaperSession {
    medium: RefCell<reaper_medium::ReaperSession>,
    logger: slog::Logger,
    // We take a mutable reference from this RefCell in order to add/remove commands.
    // TODO-low Adding an action in an action would panic because we have an immutable borrow of
    // the map  to obtain and execute the command, plus a mutable borrow of the map to add the
    // new command.  (the latter being unavoidable because we somehow need to modify the map!).
    //  That's not good. Is there a way to avoid this constellation? It's probably hard to avoid
    // the  immutable borrow because the `operation` is part of the map after all. And we can't
    // just  copy it before execution, at least not when it captures and mutates state, which
    // might not  be copyable (which we want to explicitly allow, that's why we accept FnMut!).
    // Or is it  possible to give up the map borrow after obtaining the command/operation
    // reference???  Look into that!!!
    command_by_id: RefCell<HashMap<CommandId, Command>>,
    pub(super) subjects: MainSubjects,
    undo_block_is_active: Cell<bool>,
    main_thread_task_channel: (Sender<MainThreadTask>, Receiver<MainThreadTask>),
    audio_thread_task_channel: (Sender<AudioThreadTaskOp>, Receiver<AudioThreadTaskOp>),
    active_data: RefCell<Option<ActiveData>>,
}

impl Default for ReaperSession {
    fn default() -> Self {
        ReaperSession {
            medium: Default::default(),
            logger: create_std_logger(),
            command_by_id: Default::default(),
            subjects: Default::default(),
            undo_block_is_active: Default::default(),
            main_thread_task_channel: crossbeam_channel::bounded::<MainThreadTask>(0),
            audio_thread_task_channel: crossbeam_channel::bounded::<AudioThreadTaskOp>(0),
            active_data: Default::default(),
        }
    }
}

#[derive(Debug)]
struct ActiveData {
    csurf_inst_handle: NonNull<raw::IReaperControlSurface>,
    audio_hook_register_handle: NonNull<raw::audio_hook_register_t>,
}

#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub struct MidiEvent<M> {
    frame_offset: u32,
    msg: M,
}

impl<M> MidiEvent<M> {
    pub fn new(frame_offset: u32, msg: M) -> MidiEvent<M> {
        MidiEvent { frame_offset, msg }
    }
}

#[derive(Default)]
pub(super) struct MainSubjects {
    // This is a RefCell. So calling next() while another next() is still running will panic.
    // I guess it's good that way because this is very generic code, panicking or not panicking
    // depending on the user's code. And getting a panic is good for becoming aware of the problem
    // instead of running into undefined behavior. The developer can always choose to defer to
    // the next `ControlSurface::run()` invocation (execute things in next main loop cycle).
    pub(super) project_switched: EventStreamSubject<Project>,
    pub(super) track_volume_changed: EventStreamSubject<Track>,
    pub(super) track_volume_touched: EventStreamSubject<Track>,
    pub(super) track_pan_changed: EventStreamSubject<Track>,
    pub(super) track_pan_touched: EventStreamSubject<Track>,
    pub(super) track_send_volume_changed: EventStreamSubject<TrackSend>,
    pub(super) track_send_volume_touched: EventStreamSubject<TrackSend>,
    pub(super) track_send_pan_changed: EventStreamSubject<TrackSend>,
    pub(super) track_send_pan_touched: EventStreamSubject<TrackSend>,
    pub(super) track_added: EventStreamSubject<Track>,
    pub(super) track_removed: EventStreamSubject<Track>,
    pub(super) tracks_reordered: EventStreamSubject<Project>,
    pub(super) track_name_changed: EventStreamSubject<Track>,
    pub(super) track_input_changed: EventStreamSubject<Track>,
    pub(super) track_input_monitoring_changed: EventStreamSubject<Track>,
    pub(super) track_arm_changed: EventStreamSubject<Track>,
    pub(super) track_mute_changed: EventStreamSubject<Track>,
    pub(super) track_mute_touched: EventStreamSubject<Track>,
    pub(super) track_solo_changed: EventStreamSubject<Track>,
    pub(super) track_selected_changed: EventStreamSubject<Track>,
    pub(super) fx_added: EventStreamSubject<Fx>,
    pub(super) fx_removed: EventStreamSubject<Fx>,
    pub(super) fx_enabled_changed: EventStreamSubject<Fx>,
    pub(super) fx_opened: EventStreamSubject<Fx>,
    pub(super) fx_closed: EventStreamSubject<Fx>,
    pub(super) fx_focused: EventStreamSubject<Payload<Option<Fx>>>,
    pub(super) fx_reordered: EventStreamSubject<Track>,
    pub(super) fx_parameter_value_changed: EventStreamSubject<FxParameter>,
    pub(super) fx_parameter_touched: EventStreamSubject<FxParameter>,
    pub(super) master_tempo_changed: EventStreamSubject<()>,
    pub(super) master_tempo_touched: EventStreamSubject<()>,
    pub(super) master_playrate_changed: EventStreamSubject<bool>,
    pub(super) master_playrate_touched: EventStreamSubject<bool>,
    pub(super) main_thread_idle: EventStreamSubject<bool>,
    pub(super) project_closed: EventStreamSubject<Project>,
    pub(super) action_invoked: EventStreamSubject<Payload<Rc<Action>>>,
}

impl Debug for MainSubjects {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MainSubjects").finish()
    }
}

#[derive(Clone)]
pub struct Payload<T>(pub T);

impl<T: Clone> PayloadCopy for Payload<T> {}

impl MainSubjects {
    fn new() -> MainSubjects {
        fn default<T>() -> EventStreamSubject<T> {
            RefCell::new(LocalSubject::new())
        }
        MainSubjects {
            project_switched: default(),
            track_volume_changed: default(),
            track_volume_touched: default(),
            track_pan_changed: default(),
            track_pan_touched: default(),
            track_send_volume_changed: default(),
            track_send_volume_touched: default(),
            track_send_pan_changed: default(),
            track_send_pan_touched: default(),
            track_added: default(),
            track_removed: default(),
            tracks_reordered: default(),
            track_name_changed: default(),
            track_input_changed: default(),
            track_input_monitoring_changed: default(),
            track_arm_changed: default(),
            track_mute_changed: default(),
            track_mute_touched: default(),
            track_solo_changed: default(),
            track_selected_changed: default(),
            fx_added: default(),
            fx_removed: default(),
            fx_enabled_changed: default(),
            fx_opened: default(),
            fx_closed: default(),
            fx_focused: default(),
            fx_reordered: default(),
            fx_parameter_value_changed: default(),
            fx_parameter_touched: default(),
            master_tempo_changed: default(),
            master_tempo_touched: default(),
            master_playrate_changed: default(),
            master_playrate_touched: default(),
            main_thread_idle: default(),
            project_closed: default(),
            action_invoked: default(),
        }
    }
}

pub enum ActionKind {
    NotToggleable,
    Toggleable(Box<dyn Fn() -> bool>),
}

pub fn toggleable(is_on: impl Fn() -> bool + 'static) -> ActionKind {
    Toggleable(Box::new(is_on))
}

type EventStreamSubject<T> = RefCell<LocalSubject<'static, T, ()>>;

pub struct ReaperGuard;

impl Drop for ReaperGuard {
    fn drop(&mut self) {
        ReaperSession::get().deactivate();
    }
}

impl ReaperSession {
    /// The given initializer is executed only the first time this is called and when there's no
    /// Arc sticking around anymore.
    pub fn guarded(initializer: impl FnOnce()) -> Arc<ReaperGuard> {
        // This is supposed to be called in the main thread. A check is not necessary, because this
        // is protected by a mutex and it will fail in the initializer and getter if called from
        // wrong thread.
        let mut result = REAPER_GUARD.lock().unwrap();
        if let Some(rc) = result.upgrade() {
            return rc;
        }
        initializer();
        let arc = Arc::new(ReaperGuard);
        *result = Arc::downgrade(&arc);
        arc
    }

    /// Returns the builder for further configuration of the session to be constructed.
    pub fn load(context: ReaperPluginContext) -> ReaperBuilder {
        require_main_thread(&context);
        ReaperBuilder::new(context)
    }

    /// This has an effect only if there isn't an instance already.
    pub fn setup_with_defaults(context: ReaperPluginContext, email_address: &'static str) {
        require_main_thread(&context);
        ReaperSession::load(context)
            .logger(create_terminal_logger())
            .setup();
        std::panic::set_hook(create_reaper_panic_hook(
            create_terminal_logger(),
            Some(create_default_console_msg_formatter(email_address)),
        ));
    }

    /// May be called from any thread.
    // Allowing global access to native REAPER functions at all times is valid in my opinion.
    // Because REAPER itself is not written in Rust and therefore cannot take part in Rust's compile
    // time guarantees anyway. We need to rely on REAPER at that point and also take care not to do
    // something which is not allowed in Reaper (by reading documentation and learning from
    // mistakes ... no compiler is going to save us from them). REAPER as a whole is always mutable
    // from the perspective of extensions.
    //
    // We express that in Rust by making `Reaper` class an immutable (in the sense of non-`&mut`)
    // singleton and allowing all REAPER functions to be called from an immutable context ...
    // although they can and often will lead to mutations within REAPER!
    pub fn get() -> &'static ReaperSession {
        unsafe {
            INSTANCE
                .as_ref()
                .expect("Reaper::load().setup() must be called before Reaper::get()")
        }
    }

    pub fn activate(&self) {
        self.require_main_thread();
        let mut active_data = self.active_data.borrow_mut();
        assert!(active_data.is_none(), "Reaper is already active");
        self.discard_pending_tasks();
        let real_time_reaper = self.medium().create_real_time_reaper();
        let control_surface = HelperControlSurface::new(
            self.main_thread_task_channel.0.clone(),
            self.main_thread_task_channel.1.clone(),
        );
        let mut medium = self.medium_mut();
        // Functions
        medium
            .plugin_register_add_hook_command::<HighLevelHookCommand>()
            .expect("couldn't register hook command");
        medium
            .plugin_register_add_toggle_action::<HighLevelToggleAction>()
            .expect("couldn't register toggle command");
        medium
            .plugin_register_add_hook_post_command::<HighLevelHookPostCommand>()
            .expect("couldn't register hook post command");
        // Audio hook
        let audio_hook = HighOnAudioBuffer {
            audio_thread_task_receiver: self.audio_thread_task_channel.1.clone(),
            session: RealTimeReaperSession {
                medium: real_time_reaper,
                midi_message_received: LocalSubject::new(),
                main_thread_task_sender: self.main_thread_task_channel.0.clone(),
            },
        };
        *active_data = Some(ActiveData {
            csurf_inst_handle: {
                medium
                    .plugin_register_add_csurf_inst(control_surface)
                    .unwrap()
            },
            audio_hook_register_handle: { medium.audio_reg_hardware_hook_add(audio_hook).unwrap() },
        });
    }

    pub fn deactivate(&self) {
        self.require_main_thread();
        let mut active_data = self.active_data.borrow_mut();
        let ad = match active_data.as_ref() {
            None => panic!("Reaper is not active"),
            Some(ad) => ad,
        };
        let mut medium = self.medium_mut();
        // Remove audio hook
        medium.audio_reg_hardware_hook_remove(ad.audio_hook_register_handle);
        // Remove control surface
        medium.plugin_register_remove_csurf_inst(ad.csurf_inst_handle);
        // Remove functions
        medium.plugin_register_remove_hook_post_command::<HighLevelHookPostCommand>();
        medium.plugin_register_remove_toggle_action::<HighLevelToggleAction>();
        medium.plugin_register_remove_hook_command::<HighLevelHookCommand>();
        *active_data = None;
    }

    /// We don't want to execute tasks which accumulated during the "downtime" of ReaperSession.
    /// So we just consume all without executing them.
    fn discard_pending_tasks(&self) {
        self.discard_main_thread_tasks();
        self.discard_audio_thread_tasks();
    }

    fn discard_main_thread_tasks(&self) {
        let task_count = self.main_thread_task_channel.1.try_iter().count();
        if task_count > 0 {
            slog::warn!(self.logger, "Discarded main thread tasks on reactivation";
                "task_count" => task_count,
            );
        }
    }

    fn discard_audio_thread_tasks(&self) {
        let task_count = self.audio_thread_task_channel.1.try_iter().count();
        if task_count > 0 {
            slog::warn!(self.logger, "Discarded audio thread tasks on reactivation";
                "task_count" => task_count,
            );
        }
    }

    pub fn medium(&self) -> Ref<reaper_medium::ReaperSession> {
        self.require_main_thread();
        self.medium.borrow()
    }

    pub fn medium_mut(&self) -> RefMut<reaper_medium::ReaperSession> {
        self.require_main_thread();
        self.medium.borrow_mut()
    }

    pub fn register_action(
        &self,
        command_name: &CStr,
        description: impl Into<ReaperStringArg<'static>>,
        operation: impl FnMut() + 'static,
        kind: ActionKind,
    ) -> RegisteredAction {
        self.require_main_thread();
        let mut medium = self.medium_mut();
        let command_id = medium.plugin_register_add_command_id(command_name).unwrap();
        let command = Command::new(Rc::new(RefCell::new(operation)), kind);
        if let Entry::Vacant(p) = self.command_by_id.borrow_mut().entry(command_id) {
            p.insert(command);
        }
        let address = medium
            .plugin_register_add_gaccel(MediumGaccelRegister::without_key_binding(
                command_id,
                description,
            ))
            .unwrap();
        RegisteredAction::new(command_id, address)
    }

    fn unregister_command(
        &self,
        command_id: CommandId,
        gaccel_handle: NonNull<raw::gaccel_register_t>,
    ) {
        // Unregistering command when it's destroyed via RAII (implementing Drop)? Bad idea, because
        // this is the wrong point in time. The right point in time for unregistering is when it's
        // removed from the command hash map. Because even if the command still exists in memory,
        // if it's not in the map anymore, REAPER won't be able to find it.
        let mut command_by_id = self.command_by_id.borrow_mut();
        if let Some(_command) = command_by_id.get_mut(&command_id) {
            self.medium_mut()
                .plugin_register_remove_gaccel(gaccel_handle);
            command_by_id.remove(&command_id);
        }
    }

    pub fn project_switched(&self) -> impl LocalObservable<'static, Err = (), Item = Project> {
        self.require_main_thread();
        self.subjects.project_switched.borrow().clone()
    }

    pub fn fx_opened(&self) -> impl LocalObservable<'static, Err = (), Item = Fx> {
        self.require_main_thread();
        self.subjects.fx_opened.borrow().clone()
    }

    pub fn fx_focused(
        &self,
    ) -> impl LocalObservable<'static, Err = (), Item = Payload<Option<Fx>>> {
        self.require_main_thread();
        self.subjects.fx_focused.borrow().clone()
    }

    pub fn track_added(&self) -> impl LocalObservable<'static, Err = (), Item = Track> {
        self.require_main_thread();
        self.subjects.track_added.borrow().clone()
    }

    // Delivers a GUID-based track (to still be able to identify it even it is deleted)
    pub fn track_removed(&self) -> impl LocalObservable<'static, Err = (), Item = Track> {
        self.require_main_thread();
        self.subjects.track_removed.borrow().clone()
    }

    pub fn track_name_changed(&self) -> impl LocalObservable<'static, Err = (), Item = Track> {
        self.require_main_thread();
        self.subjects.track_name_changed.borrow().clone()
    }

    pub fn master_tempo_changed(&self) -> impl LocalObservable<'static, Err = (), Item = ()> {
        self.require_main_thread();
        self.subjects.master_tempo_changed.borrow().clone()
    }

    pub fn fx_added(&self) -> impl LocalObservable<'static, Err = (), Item = Fx> {
        self.require_main_thread();
        self.subjects.fx_added.borrow().clone()
    }

    pub fn fx_enabled_changed(&self) -> impl LocalObservable<'static, Err = (), Item = Fx> {
        self.require_main_thread();
        self.subjects.fx_enabled_changed.borrow().clone()
    }

    pub fn fx_reordered(&self) -> impl LocalObservable<'static, Err = (), Item = Track> {
        self.require_main_thread();
        self.subjects.fx_reordered.borrow().clone()
    }

    pub fn fx_removed(&self) -> impl LocalObservable<'static, Err = (), Item = Fx> {
        self.require_main_thread();
        self.subjects.fx_removed.borrow().clone()
    }

    pub fn fx_parameter_value_changed(
        &self,
    ) -> impl LocalObservable<'static, Err = (), Item = FxParameter> {
        self.require_main_thread();
        self.subjects.fx_parameter_value_changed.borrow().clone()
    }

    pub fn track_input_monitoring_changed(
        &self,
    ) -> impl LocalObservable<'static, Err = (), Item = Track> {
        self.require_main_thread();
        self.subjects
            .track_input_monitoring_changed
            .borrow()
            .clone()
    }

    pub fn track_input_changed(&self) -> impl LocalObservable<'static, Err = (), Item = Track> {
        self.require_main_thread();
        self.subjects.track_input_changed.borrow().clone()
    }

    pub fn track_volume_changed(&self) -> impl LocalObservable<'static, Err = (), Item = Track> {
        self.require_main_thread();
        self.subjects.track_volume_changed.borrow().clone()
    }

    pub fn track_pan_changed(&self) -> impl LocalObservable<'static, Err = (), Item = Track> {
        self.require_main_thread();
        self.subjects.track_pan_changed.borrow().clone()
    }

    pub fn track_selected_changed(&self) -> impl LocalObservable<'static, Err = (), Item = Track> {
        self.require_main_thread();
        self.subjects.track_selected_changed.borrow().clone()
    }

    pub fn track_mute_changed(&self) -> impl LocalObservable<'static, Err = (), Item = Track> {
        self.require_main_thread();
        self.subjects.track_mute_changed.borrow().clone()
    }

    pub fn track_solo_changed(&self) -> impl LocalObservable<'static, Err = (), Item = Track> {
        self.require_main_thread();
        self.subjects.track_solo_changed.borrow().clone()
    }

    pub fn track_arm_changed(&self) -> impl LocalObservable<'static, Err = (), Item = Track> {
        self.require_main_thread();
        self.subjects.track_arm_changed.borrow().clone()
    }

    pub fn track_send_volume_changed(
        &self,
    ) -> impl LocalObservable<'static, Err = (), Item = TrackSend> {
        self.require_main_thread();
        self.subjects.track_send_volume_changed.borrow().clone()
    }

    pub fn track_send_pan_changed(
        &self,
    ) -> impl LocalObservable<'static, Err = (), Item = TrackSend> {
        self.require_main_thread();
        self.subjects.track_send_pan_changed.borrow().clone()
    }

    pub fn action_invoked(
        &self,
    ) -> impl LocalObservable<'static, Err = (), Item = Payload<Rc<Action>>> {
        self.require_main_thread();
        self.subjects.action_invoked.borrow().clone()
    }

    // Thread-safe. Returns an error if task queue if full (typically if ReaperSession has been
    // deactivated).
    pub fn execute_in_main_thread(
        &self,
        waiting_time: Duration,
        op: impl FnOnce() + 'static,
    ) -> Result<(), ()> {
        let sender = &self.main_thread_task_channel.0;
        sender
            .send(MainThreadTask::new(
                Box::new(op),
                Some(SystemTime::now() + waiting_time),
            ))
            .map_err(|_| ())
    }

    // Thread-safe. Returns an error if task queue if full (typically if ReaperSession has been
    // deactivated).
    pub fn execute_asap_in_main_thread(&self, op: impl FnOnce() + 'static) -> Result<(), ()> {
        let sender = &self.main_thread_task_channel.0;
        sender
            .send(MainThreadTask::new(Box::new(op), None))
            .map_err(|_| ())
    }

    // Thread-safe. Returns an error if task queue if full (typically if ReaperSession has been
    // deactivated).
    pub fn execute_asap_in_audio_thread(
        &self,
        op: impl FnOnce(&RealTimeReaperSession) + 'static,
    ) -> Result<(), ()> {
        let sender = &self.audio_thread_task_channel.0;
        sender.send(Box::new(op)).map_err(|_| ())
    }

    pub fn undoable_action_is_running(&self) -> bool {
        self.require_main_thread();
        self.undo_block_is_active.get()
    }

    // Doesn't start a new block if we already are in an undo block.
    #[must_use = "Return value determines the scope of the undo block (RAII)"]
    pub(super) fn enter_undo_block_internal<'a>(
        &self,
        project: Project,
        label: &'a CStr,
    ) -> Option<UndoBlock<'a>> {
        self.require_main_thread();
        if self.undo_block_is_active.get() {
            return None;
        }
        self.undo_block_is_active.replace(true);
        self.medium()
            .reaper()
            .undo_begin_block_2(Proj(project.get_raw()));
        Some(UndoBlock::new(project, label))
    }

    // Doesn't attempt to end a block if we are not in an undo block.
    pub(super) fn leave_undo_block_internal(&self, project: Project, label: &CStr) {
        self.require_main_thread();
        if !self.undo_block_is_active.get() {
            return;
        }
        self.medium()
            .reaper()
            .undo_end_block_2(Proj(project.get_raw()), label, All);
        self.undo_block_is_active.replace(false);
    }

    fn require_main_thread(&self) {
        require_main_thread(Reaper::get().medium().low().plugin_context());
    }
}

unsafe impl Sync for ReaperSession {}

struct Command {
    /// Reasoning for that type (from inner to outer):
    /// - `FnMut`: We don't use just `fn` because we want to support closures. We don't use just
    ///   `Fn` because we want to support closures that keep mutable references to their captures.
    ///   We can't accept `FnOnce` because that would mean that the closure value itself is
    ///   consumed when it's called. That means we would have to remove the action from the action
    ///   list just to call it and we couldn't again it again.
    /// - `Box`: Of course we want to support very different closures with very different captures.
    ///   We don't use generic type parameters to achieve that because we need to put Commands into
    ///   a HashMap as values - so we need each Command to have the same size in memory and the
    ///   same type. Generics lead to the generation of different types and most likely also
    ///   different sizes. We don't use references because we want ownership. Yes, Box is (like
    ///   reference) a so-called trait object and therefore uses dynamic dispatch. It also needs
    ///   heap allocation (unlike general references). However, this is exactly what we want and
    ///   need here.
    /// - `RefCell`: We need this in order to make the FnMut callable in immutable context (for
    ///   safety reasons we are mostly in immutable context, see ControlSurface documentation).
    ///   It's good to use `RefCell` in a very fine-grained way like that and not for example on
    ///   the whole `Command`. That allows for very localized mutation and therefore a lower
    ///   likelihood that borrowing rules are violated (or if we wouldn't have the runtime borrow
    ///   checking of `RefCell`, the likeliness to get undefined behavior).
    /// - `Rc`: We don't want to keep an immutable reference to the surrounding `Command` around
    ///   just in order to execute this operation! Why? Because we want to support operations which
    ///   add a REAPER action when executed. And when doing that, we of course have to borrow the
    ///   command HashMap mutably. However, at that point we already have an immutable borrow to
    ///   the complete HashMap (via a `RefCell`) ... boom. Panic! With the `Rc` we can release the
    ///   borrow by cloning the first `Rc` instance and therefore gaining a short-term second
    ///   ownership of that operation.
    /// - Wait ... actually there's no `Box` anymore! Turned out that `Rc` makes all things
    ///   possible that also `Box` makes possible, in particular taking dynamically-sized types. If
    ///   we wouldn't need `Rc` (for shared references), we would have to take `Box` instead.
    operation: Rc<RefCell<dyn FnMut()>>,
    kind: ActionKind,
}

impl Debug for Command {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Command").finish()
    }
}

impl Command {
    fn new(operation: Rc<RefCell<dyn FnMut()>>, kind: ActionKind) -> Command {
        Command { operation, kind }
    }
}

pub struct RegisteredAction {
    // For identifying the registered command (= the functions to be executed)
    command_id: CommandId,
    // For identifying the registered action (= description, related keyboard shortcuts etc.)
    gaccel_handle: NonNull<raw::gaccel_register_t>,
}

impl RegisteredAction {
    fn new(
        command_id: CommandId,
        gaccel_handle: NonNull<raw::gaccel_register_t>,
    ) -> RegisteredAction {
        RegisteredAction {
            command_id,
            gaccel_handle,
        }
    }

    pub fn unregister(&self) {
        require_main_thread(Reaper::get().medium().low().plugin_context());
        ReaperSession::get().unregister_command(self.command_id, self.gaccel_handle);
    }
}

// Called by REAPER (using a delegate function)!
// Only for main section
struct HighLevelHookCommand {}

impl MediumHookCommand for HighLevelHookCommand {
    fn call(command_id: CommandId, _flag: i32) -> bool {
        // TODO-low Pass on flag
        let operation = match ReaperSession::get().command_by_id.borrow().get(&command_id) {
            Some(command) => command.operation.clone(),
            None => return false,
        };
        (*operation).borrow_mut().call_mut(());
        true
    }
}

// Called by REAPER directly (using a delegate function)!
// Only for main section
struct HighLevelHookPostCommand {}

impl MediumHookPostCommand for HighLevelHookPostCommand {
    fn call(command_id: CommandId, _flag: i32) {
        let action = Reaper::get()
            .get_main_section()
            .get_action_by_command_id(command_id);
        ReaperSession::get()
            .subjects
            .action_invoked
            .borrow_mut()
            .next(Payload(Rc::new(action)));
    }
}

// Called by REAPER directly!
// Only for main section
struct HighLevelToggleAction {}

impl MediumToggleAction for HighLevelToggleAction {
    fn call(command_id: CommandId) -> ToggleActionResult {
        if let Some(command) = ReaperSession::get()
            .command_by_id
            .borrow()
            .get(&(command_id))
        {
            match &command.kind {
                ActionKind::Toggleable(is_on) => {
                    if is_on() {
                        ToggleActionResult::On
                    } else {
                        ToggleActionResult::Off
                    }
                }
                ActionKind::NotToggleable => ToggleActionResult::NotRelevant,
            }
        } else {
            ToggleActionResult::NotRelevant
        }
    }
}

type AudioThreadTaskOp = Box<dyn FnOnce(&RealTimeReaperSession) + 'static>;

type MainThreadTaskOp = Box<dyn FnOnce() + 'static>;

pub(super) struct MainThreadTask {
    pub desired_execution_time: Option<std::time::SystemTime>,
    pub op: MainThreadTaskOp,
}

impl MainThreadTask {
    pub fn new(
        op: MainThreadTaskOp,
        desired_execution_time: Option<std::time::SystemTime>,
    ) -> MainThreadTask {
        MainThreadTask {
            desired_execution_time,
            op,
        }
    }
}

fn require_main_thread(context: &ReaperPluginContext) {
    assert!(
        context.is_in_main_thread(),
        "this function must be called in the main thread"
    );
}
