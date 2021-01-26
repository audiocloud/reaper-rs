#![allow(renamed_and_removed_lints)]
use enumflags2::BitFlags;
use reaper_low::raw;

/// When creating an undo point, this defines what parts of the project might have been affected by
/// the undoable operation.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum UndoScope {
    /// Everything could have been affected.
    ///
    /// This is the safest variant but can lead to very large undo states.
    All,
    /// A combination of the given project parts could have been affected.
    ///
    /// If you miss some parts, *undo* can behave in weird ways.
    Scoped(BitFlags<ProjectPart>),
}

impl UndoScope {
    /// Converts this value to an integer as expected by the low-level API.
    pub fn to_raw(self) -> i32 {
        use UndoScope::*;
        match self {
            All => raw::UNDO_STATE_ALL as i32,
            Scoped(flags) => flags.bits() as i32,
        }
    }
}

/// Part of a project that could have been affected by an undoable operation.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, BitFlags)]
#[repr(u32)]
pub enum ProjectPart {
    /// Freeze state.
    Freeze = raw::UNDO_STATE_FREEZE,
    /// Track master FX.
    Fx = raw::UNDO_STATE_FX,
    /// Track items.
    Items = raw::UNDO_STATE_ITEMS,
    /// Loop selection, markers, regions and extensions.
    MiscCfg = raw::UNDO_STATE_MISCCFG,
    /// Track/master vol/pan/routing and aLL envelopes (master included).
    TrackCfg = raw::UNDO_STATE_TRACKCFG,
}

/// Activates certain behaviors when inserting a media file.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, BitFlags)]
#[repr(u32)]
pub enum InsertMediaFlag {
    StretchLoopToFitTimeSelection = 4,
    TryToMatchTempo1X = 8,
    TryToMatchTempo05X = 16,
    TryToMatchTempo2X = 32,
    DontPreservePitchWhenMatchingTempo = 64,
    NoLoopSectionIfStartPctEndPctSet = 128,
    /// Force loop regardless of global preference for looping imported items.
    ForceLoopRegardlessOfGlobalPreference = 256,
    /// Move to source preferred position (BWF start offset).
    MoveSourceToPreferredPosition = 4096,
}
