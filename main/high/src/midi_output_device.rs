use crate::Reaper;
use reaper_medium::{MidiOutputDeviceId, ReaperString};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct MidiOutputDevice {
    id: MidiOutputDeviceId,
}

impl MidiOutputDevice {
    pub fn new(id: MidiOutputDeviceId) -> Self {
        MidiOutputDevice { id }
    }

    pub fn id(self) -> MidiOutputDeviceId {
        self.id
    }

    pub fn name(self) -> ReaperString {
        Reaper::get()
            .medium_reaper()
            .get_midi_output_name(self.id, 33)
            .name
            .unwrap()
    }

    // For REAPER < 5.94 this is the same like isConnected(). For REAPER >=5.94 it returns true if
    // the device ever existed, even if it's disconnected now.
    pub fn is_available(self) -> bool {
        let result = Reaper::get()
            .medium_reaper()
            .get_midi_output_name(self.id, 2);
        result.is_present || result.name.is_some()
    }

    // Only returns true if the device is connected (= present)
    pub fn is_connected(self) -> bool {
        // In REAPER 5.94 GetMIDIOutputName doesn't accept nullptr as name buffer on OS X
        Reaper::get()
            .medium_reaper()
            .get_midi_output_name(self.id, 1)
            .is_present
    }
}
