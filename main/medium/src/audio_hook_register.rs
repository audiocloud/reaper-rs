use crate::{decode_user_data, encode_user_data, Hz};
use reaper_low::raw::audio_hook_register_t;
use reaper_low::{firewall, raw};

use std::fmt;
use std::fmt::Debug;
use std::os::raw::c_int;
use std::ptr::{null_mut, NonNull};

/// Consumers need to implement this trait in order to be called back in the real-time audio thread.
///
/// See [`audio_reg_hardware_hook_add()`].
///
/// [`audio_reg_hardware_hook_add()`]: struct.ReaperSession.html#method.audio_reg_hardware_hook_add
pub trait OnAudioBuffer {
    /// The actual callback function.
    ///
    /// It's called twice per frame, first with `is_post` being `false`, then `true`.
    fn call(&mut self, args: OnAudioBufferArgs);
}

#[derive(PartialEq, Debug)]
pub struct OnAudioBufferArgs<'a> {
    pub is_post: bool,
    pub len: u32,
    pub srate: Hz,
    pub reg: &'a AudioHookRegister,
}

/// Pointer to an audio hook register.
///
/// In future this should provides access to the current audio buffer contents.
// Case 2: Internals exposed: yes | vtable: no
// ===========================================
//
// It's important that this type is not cloneable! Otherwise consumers could easily let it escape
// its intended usage scope (audio hook), which would lead to undefined behavior.
//
// We don't expose the user-defined data pointers. The first one is already exposed implicitly as
// `&mut self` in the callback function. The second one is unnecessary.
#[derive(Eq, PartialEq, Hash, Debug)]
pub struct AudioHookRegister(pub NonNull<raw::audio_hook_register_t>);

impl AudioHookRegister {
    pub(crate) fn new(ptr: NonNull<raw::audio_hook_register_t>) -> AudioHookRegister {
        AudioHookRegister(ptr)
    }

    /// Returns the raw pointer.
    pub fn get(&self) -> NonNull<raw::audio_hook_register_t> {
        self.0
    }

    /// Returns the current number of input channels.
    pub fn input_nch(&self) -> u32 {
        unsafe { self.0.as_ref() }.input_nch as u32
    }

    /// Returns the current number of output channels.
    pub fn output_nch(&self) -> u32 {
        unsafe { self.0.as_ref() }.input_nch as u32
    }
}

extern "C" fn delegating_on_audio_buffer<T: OnAudioBuffer>(
    is_post: bool,
    len: c_int,
    srate: f64,
    reg: *mut audio_hook_register_t,
) {
    // TODO-low Check performance implications for firewall call
    firewall(|| {
        let reg = unsafe { NonNull::new_unchecked(reg) };
        let callback_struct: &mut T = decode_user_data(unsafe { reg.as_ref() }.userdata1);
        callback_struct.call(OnAudioBufferArgs {
            is_post,
            len: len as u32,
            srate: unsafe { Hz::new_unchecked(srate) },
            reg: &AudioHookRegister::new(reg),
        });
    });
}

pub(crate) struct OwnedAudioHookRegister {
    inner: raw::audio_hook_register_t,
    callback: Box<dyn OnAudioBuffer>,
}

impl Debug for OwnedAudioHookRegister {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Besides OnAudioBuffer not generally implementing Debug, it would also be a bit dangerous.
        // Debug-printing the REAPER session could cause race conditions when the debug formatting
        // accesses audio hook state.
        f.debug_struct("OwnedAudioHookRegister")
            .field("inner", &self.inner)
            .field("callback", &"<omitted>")
            .finish()
    }
}

impl OwnedAudioHookRegister {
    /// Creates an audio hook register.
    ///
    /// See [`audio_reg_hardware_hook_add`].
    ///
    /// # Design
    ///
    /// Taking ownership of the user-defined piece of data releases the API consumer of the burden
    /// of maintaining a stable memory address and ensuring correct lifetime.
    ///
    /// [`audio_reg_hardware_hook_add`]:
    /// struct.ReaperSession.html#method.audio_reg_hardware_hook_add
    pub fn new<T>(callback: Box<T>) -> OwnedAudioHookRegister
    where
        T: OnAudioBuffer + 'static,
    {
        OwnedAudioHookRegister {
            inner: audio_hook_register_t {
                OnAudioBuffer: Some(delegating_on_audio_buffer::<T>),
                // boxed_callback_struct is not a fat pointer. Even if it would be, thanks to
                // generics the callback knows what's the concrete type and therefore can restore
                // the original type correctly without needing the vtable part of the fat
                // pointer.
                userdata1: encode_user_data(&callback),
                userdata2: null_mut(),
                input_nch: 0,
                output_nch: 2,
                GetBuffer: None,
            },
            callback,
        }
    }

    pub fn into_callback(self) -> Box<dyn OnAudioBuffer> {
        self.callback
    }
}

impl AsRef<raw::audio_hook_register_t> for OwnedAudioHookRegister {
    fn as_ref(&self) -> &audio_hook_register_t {
        &self.inner
    }
}
