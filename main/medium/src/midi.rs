use helgoboss_midi::{ShortMessage, U7};
use reaper_low::raw;

use crate::{MidiFrameOffset, SendMidiTime};
use reaper_low::raw::MIDI_event_t;
use ref_cast::RefCast;
use std::os::raw::c_int;
use std::ptr::NonNull;

/// Pointer to a MIDI input device.
//
// Case 3: Internals exposed: no | vtable: yes
// ===========================================
//
// It's important that this type is not cloneable! Otherwise consumers could easily let it escape
// its intended usage scope (audio hook), which would lead to undefined behavior.
//
// Internals exposed: no | vtable: yes (Rust => REAPER)
#[derive(Eq, PartialEq, Hash, Debug)]
pub struct MidiInput(pub(crate) NonNull<raw::midi_Input>);

impl MidiInput {
    /// Returns the list of MIDI events which are currently in the buffer.
    ///
    /// This must only be called in the real-time audio thread! See [`get_midi_input()`].
    ///
    /// # Design
    ///
    /// In the past this function was unsafe and expected a closure which let the consumer do
    /// something with the event list. All of that is not necessary anymore since we ensure in
    /// [`get_midi_input()`] that we only ever publish valid [`MidiInput`] instances, and those only
    /// by a very short-lived reference that's not possible to cache anywhere. That makes it
    /// possible to bind the lifetime of the event list to the one of the [`MidiInput`] and
    /// everything is fine!
    ///
    /// Returning an owned event list would be wasteful because we would need to copy all events
    /// first. That would be especially bad because this code is supposed to run in the audio
    /// thread and therefore has real-time requirements.
    ///
    /// [`MidiInput`]: struct.MidiInput.html
    /// [`get_midi_input()`]: struct.Reaper.html#method.get_midi_input
    pub fn get_read_buf(&mut self) -> &mut BorrowedMidiEventList {
        let raw_evt_list = unsafe { (*self.0.as_ptr()).GetReadBuf() };
        if raw_evt_list.is_null() {
            panic!("GetReadBuf returned null");
        }
        unsafe { &mut *(raw_evt_list as *mut BorrowedMidiEventList) }
    }
}

/// A list of MIDI events borrowed from REAPER.
//
// Internals exposed: no | vtable: yes (Rust => REAPER)
#[derive(Eq, PartialEq, Hash, Debug, RefCast)]
#[repr(transparent)]
pub struct BorrowedMidiEventList(pub(crate) raw::MIDI_eventlist);

#[derive(Eq, PartialEq, Debug)]
pub struct EnumItemsResult<T> {
    /// MIDI event.
    pub midi_event: T,
    /// Start position of the next event in bytes.
    pub next_bpos: u32,
}

impl<T: RefCast<From = MIDI_event_t>> EnumItemsResult<&T> {
    fn from_internal(internal_res: InternalEnumItemsResult) -> Self {
        EnumItemsResult {
            midi_event: { unsafe { T::ref_cast(internal_res.midi_event.as_ref()) } },
            next_bpos: internal_res.next_bpos,
        }
    }
}

impl<T: RefCast<From = MIDI_event_t>> EnumItemsResult<&mut T> {
    fn from_internal_mut(mut internal_res: InternalEnumItemsResult) -> Self {
        EnumItemsResult {
            midi_event: { unsafe { T::ref_cast_mut(internal_res.midi_event.as_mut()) } },
            next_bpos: internal_res.next_bpos,
        }
    }
}

struct InternalEnumItemsResult {
    /// Pointer to MIDI event.
    pub midi_event: NonNull<raw::MIDI_event_t>,
    /// Start position of the next event in bytes.
    pub next_bpos: u32,
}

impl BorrowedMidiEventList {
    /// Returns the raw pointer.
    pub fn as_ptr(&self) -> NonNull<raw::MIDI_eventlist> {
        NonNull::from(&self.0)
    }

    /// Returns the MIDI event at the given byte index along with the byte index of the
    /// next event.
    pub fn enum_items(&self, bpos: u32) -> Option<EnumItemsResult<&MidiEvent>> {
        let internal_res = self.enum_items_internal(bpos)?;
        Some(EnumItemsResult::from_internal(internal_res))
    }

    /// Returns the MIDI event at the given byte index along with the byte index of the
    /// next event.
    pub fn enum_items_mut(&mut self, bpos: u32) -> Option<EnumItemsResult<&mut MidiEvent>> {
        let internal_res = self.enum_items_internal(bpos)?;
        Some(EnumItemsResult::from_internal_mut(internal_res))
    }

    pub fn iter(&self) -> EnumItems<&BorrowedMidiEventList> {
        EnumItems {
            list: self,
            bpos: 0,
        }
    }

    pub fn iter_mut(&mut self) -> EnumItems<&mut BorrowedMidiEventList> {
        EnumItems {
            list: self,
            bpos: 0,
        }
    }

    fn enum_items_internal(&self, bpos: u32) -> Option<InternalEnumItemsResult> {
        let mut bpos_int = bpos as c_int;
        let raw_evt = unsafe {
            self.as_ptr()
                .as_mut()
                .EnumItems(&mut bpos_int as *mut c_int)
        };
        let res = InternalEnumItemsResult {
            // No MIDI events left if this fails.
            midi_event: NonNull::new(raw_evt)?,
            next_bpos: bpos_int as _,
        };
        Some(res)
    }

    /// Adds an item to this list of MIDI events.
    pub fn add_item(&mut self, msg: &MidiEvent) {
        unsafe {
            self.0.AddItem(&msg.0 as *const _ as _);
        }
    }

    /// Deletes an item from this list of MIDI events.
    ///
    /// `bpos` is the byte index (not the index of the item!).
    pub fn delete_item(&mut self, bpos: u32) {
        unsafe {
            self.0.DeleteItem(bpos as _);
        }
    }

    /// Returns the size of the block in bytes.
    pub fn get_size(&self) -> u32 {
        unsafe { self.as_ptr().as_mut().GetSize() as _ }
    }

    /// Completely clears this list of MIDI events.
    pub fn empty(&mut self) {
        unsafe {
            self.0.Empty();
        }
    }
}

impl<'a> IntoIterator for &'a BorrowedMidiEventList {
    type Item = &'a MidiEvent;
    type IntoIter = EnumItems<&'a BorrowedMidiEventList>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut BorrowedMidiEventList {
    type Item = &'a mut MidiEvent;
    type IntoIter = EnumItems<&'a mut BorrowedMidiEventList>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// An owned or borrowed MIDI event for or from REAPER.
///
/// Cannot own more than a short MIDI message (just like the low-level equivalent).
//
// TODO-medium Support at least reading larger sizes of MIDI messages (by checking the size and
//  unsafely returning a slice).
// # Internals exposed: yes | vtable: no
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, RefCast)]
#[repr(transparent)]
pub struct MidiEvent(raw::MIDI_event_t);

/// An MIDI message borrowed from a REAPER MIDI event.
///
/// Can also be owned but contains more information than necessary because it contains also event
/// data.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, RefCast)]
#[repr(transparent)]
pub struct MidiMessage(raw::MIDI_event_t);

impl MidiEvent {
    /// Turns the given owned low-level MIDI event into a medium-level one.
    pub fn from_raw(raw: raw::MIDI_event_t) -> MidiEvent {
        Self(raw)
    }

    /// Turns the given low-level MIDI event reference into a medium-level one.
    pub fn from_raw_ref(raw: &raw::MIDI_event_t) -> &MidiEvent {
        MidiEvent::ref_cast(raw)
    }

    /// Returns the frame offset.
    pub fn frame_offset(&self) -> MidiFrameOffset {
        MidiFrameOffset::new(self.0.frame_offset as u32)
    }

    /// Sets the frame offset.
    pub fn set_frame_offset(&mut self, offset: MidiFrameOffset) {
        self.0.frame_offset = offset.to_raw();
    }

    /// Returns the actual message.
    pub fn message(&self) -> &MidiMessage {
        MidiMessage::ref_cast(&self.0)
    }

    /// Sets the actual message.
    pub fn set_message(&mut self, message: impl ShortMessage) {
        let bytes = message.to_bytes();
        self.0.size = 3;
        self.0.midi_message = [bytes.0, bytes.1.into(), bytes.2.into(), 0];
    }
}

impl AsRef<raw::MIDI_event_t> for MidiEvent {
    fn as_ref(&self) -> &MIDI_event_t {
        &self.0
    }
}

/// An owned MIDI event which can hold more than just the usual 3-byte short MIDI message.
///
/// Has exactly the same layout as [`MidiEvent`](struct.MidiEvent.html) but reserves much more space
/// for the message.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(C)]
pub struct LongMidiEvent {
    frame_offset: i32,
    size: i32,
    midi_message: [u8; LongMidiEvent::MAX_LENGTH],
}

impl LongMidiEvent {
    /// The maximum message length.
    // TODO-medium What's a good maximum value? This seems too low. Attention: An array of that size
    // will be created!
    pub const MAX_LENGTH: usize = 256;

    /// Creates a long MIDI event directly from an owned byte array.
    ///
    /// Size needs to be given because the actual message length is probably lower than the maximum
    /// size of a long message.  
    pub fn new(
        frame_offset: MidiFrameOffset,
        midi_message: [u8; Self::MAX_LENGTH],
        size: u32,
    ) -> Self {
        Self {
            frame_offset: frame_offset.to_raw(),
            size: size as _,
            midi_message,
        }
    }

    /// Attempts to create a long MIDI event from the given slice.
    ///
    /// Involves copying.
    ///
    /// # Errors
    ///
    /// Returns an error if the given slice is longer than the supported maximum.
    pub fn try_from_slice(
        frame_offset: MidiFrameOffset,
        midi_message: &[u8],
    ) -> Result<Self, &'static str> {
        if midi_message.len() > Self::MAX_LENGTH {
            return Err("given MIDI message too long");
        }
        let mut array = [0; Self::MAX_LENGTH];
        array[..midi_message.len()].copy_from_slice(midi_message);
        Ok(Self::new(frame_offset, array, midi_message.len() as _))
    }

    /// Returns the contained MIDI data as byte slice.
    pub fn bytes(&self) -> &[u8] {
        &self.midi_message[..self.size as usize]
    }
}

impl AsRef<raw::MIDI_event_t> for LongMidiEvent {
    fn as_ref(&self) -> &raw::MIDI_event_t {
        unsafe { &*(self as *const LongMidiEvent as *const reaper_low::raw::MIDI_event_t) }
    }
}

/// MIDI event list iterator.
pub struct EnumItems<T> {
    list: T,
    bpos: u32,
}

impl<'a> Iterator for EnumItems<&'a BorrowedMidiEventList> {
    type Item = &'a MidiEvent;

    fn next(&mut self) -> Option<Self::Item> {
        let internal_result = self.list.enum_items_internal(self.bpos)?;
        let result = EnumItemsResult::from_internal(internal_result);
        self.bpos = result.next_bpos;
        Some(result.midi_event)
    }
}

impl<'a> Iterator for EnumItems<&'a mut BorrowedMidiEventList> {
    type Item = &'a mut MidiEvent;

    fn next(&mut self) -> Option<Self::Item> {
        let internal_result = self.list.enum_items_internal(self.bpos)?;
        let result = EnumItemsResult::from_internal_mut(internal_result);
        self.bpos = result.next_bpos;
        Some(result.midi_event)
    }
}

impl MidiMessage {
    /// Returns all contained MIDI bytes as slice.
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.0.midi_message.as_ptr(), self.0.size as _) }
    }
}

impl ShortMessage for MidiMessage {
    fn status_byte(&self) -> u8 {
        self.0.midi_message[0]
    }

    fn data_byte_1(&self) -> U7 {
        unsafe { U7::new_unchecked(self.0.midi_message[1]) }
    }

    fn data_byte_2(&self) -> U7 {
        unsafe { U7::new_unchecked(self.0.midi_message[2]) }
    }
}

/// Pointer to a MIDI output device.
//
// Case 3: Internals exposed: no | vtable: yes
// ===========================================
//
// It's important that this type is not cloneable! Otherwise consumers could easily let it escape
// its intended usage scope (audio hook), which would lead to undefined behavior.
//
// Internals exposed: no | vtable: yes (Rust => REAPER)
#[derive(Eq, PartialEq, Hash, Debug)]
pub struct MidiOutput(pub(crate) NonNull<raw::midi_Output>);

impl MidiOutput {
    /// Sends the given arbitrary MIDI message to this device at the given time.
    ///
    /// This must only be called in the real-time audio thread! See [`get_midi_output()`].
    ///
    /// [`get_midi_output()`]: struct.Reaper.html#method.get_midi_output
    pub fn send_msg(&self, msg: impl AsRef<raw::MIDI_event_t>, time: SendMidiTime) {
        unsafe {
            self.0
                .as_ref()
                .SendMsg(msg.as_ref() as *const _ as _, time.to_raw());
        }
    }

    /// Sends the given short message to this device at the given time.
    ///
    /// This must only be called in the real-time audio thread! See [`get_midi_output()`].
    ///
    /// [`get_midi_output()`]: struct.Reaper.html#method.get_midi_output
    pub fn send(&self, message: impl ShortMessage, time: SendMidiTime) {
        let bytes = message.to_bytes();
        unsafe {
            self.0
                .as_ref()
                .Send(bytes.0, bytes.1.get(), bytes.2.get(), time.to_raw());
        }
    }
}
