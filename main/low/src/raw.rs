//! Exposes important raw types, functions and constants from the C++ REAPER API.

use std::os::raw::c_int;

/// Structs, types and constants defined by REAPER.
pub use super::bindings::root::{
    audio_hook_register_t, gaccel_register_t, midi_Input, midi_Output, reaper_plugin_info_t,
    IReaperControlSurface, KbdCmd, KbdSectionInfo, MIDI_event_t, MIDI_eventlist, MediaItem,
    MediaItem_Take, MediaTrack, PCM_source, ReaProject, ReaSample, TrackEnvelope,
    CSURF_EXT_SETBPMANDPLAYRATE, CSURF_EXT_SETFOCUSEDFX, CSURF_EXT_SETFXCHANGE,
    CSURF_EXT_SETFXENABLED, CSURF_EXT_SETFXOPEN, CSURF_EXT_SETFXPARAM, CSURF_EXT_SETFXPARAM_RECFX,
    CSURF_EXT_SETINPUTMONITOR, CSURF_EXT_SETLASTTOUCHEDFX, CSURF_EXT_SETSENDPAN,
    CSURF_EXT_SETSENDVOLUME, CSURF_EXT_TRACKFX_PRESET_CHANGED, REAPER_PLUGIN_VERSION,
    UNDO_STATE_ALL, UNDO_STATE_FREEZE, UNDO_STATE_FX, UNDO_STATE_ITEMS, UNDO_STATE_MISCCFG,
    UNDO_STATE_TRACKCFG,
};

/// Structs, types and constants defined by `swell.h` (on Linux and Mac OS X) and
/// `windows.h` (on Windows).
///
/// When exposing a Windows API struct/types or a REAPER struct which contains Windows API
/// structs/types, it would be good to recheck if its binary representation is the same
/// in the Windows-generated `bindings.rs` (based on `windows.h`) as in the
/// Linux-generated `bindings.rs` (based on `swell.h`). If not, that can introduce
/// cross-platform issues.
///
/// It seems SWELL itself does a pretty good job already to keep the binary representations
/// the same. E.g. `DWORD` ends up as `c_ulong` on Windows (= `u32` on Windows) and
/// `c_uint` on Linux (= `u32` on Linux).
pub use super::bindings::root::{
    ACCEL, BM_GETCHECK, BM_GETIMAGE, BM_SETCHECK, BM_SETIMAGE, BST_CHECKED, BST_INDETERMINATE,
    BST_UNCHECKED, CBN_CLOSEUP, CBN_DROPDOWN, CBN_EDITCHANGE, CBN_SELCHANGE, CB_ADDSTRING,
    CB_DELETESTRING, CB_FINDSTRING, CB_FINDSTRINGEXACT, CB_GETCOUNT, CB_GETCURSEL, CB_GETITEMDATA,
    CB_GETLBTEXT, CB_GETLBTEXTLEN, CB_INITSTORAGE, CB_INSERTSTRING, CB_RESETCONTENT, CB_SETCURSEL,
    CB_SETITEMDATA, DLL_PROCESS_ATTACH, EN_CHANGE, EN_KILLFOCUS, EN_SETFOCUS, GMEM_DDESHARE,
    GMEM_DISCARDABLE, GMEM_FIXED, GMEM_LOWER, GMEM_MOVEABLE, GMEM_SHARE, GMEM_ZEROINIT, GUID,
    HANDLE, HINSTANCE, HWND, HWND__, IDABORT, IDCANCEL, IDIGNORE, IDNO, IDOK, IDRETRY, IDYES,
    INT_PTR, LPARAM, LPSTR, LRESULT, SB_BOTH, SB_BOTTOM, SB_CTL, SB_ENDSCROLL, SB_HORZ, SB_LEFT,
    SB_LINEDOWN, SB_LINELEFT, SB_LINERIGHT, SB_LINEUP, SB_PAGEDOWN, SB_PAGELEFT, SB_PAGERIGHT,
    SB_PAGEUP, SB_RIGHT, SB_THUMBPOSITION, SB_THUMBTRACK, SB_TOP, SB_VERT, SCROLLINFO, SIF_ALL,
    SIF_DISABLENOSCROLL, SIF_PAGE, SIF_POS, SIF_RANGE, SIF_TRACKPOS, UINT, ULONG_PTR, VK_CONTROL,
    VK_MENU, VK_SHIFT, WM_ACTIVATE, WM_ACTIVATEAPP, WM_CAPTURECHANGED, WM_CHAR, WM_CLOSE,
    WM_COMMAND, WM_CONTEXTMENU, WM_COPYDATA, WM_CREATE, WM_DEADCHAR, WM_DESTROY, WM_DISPLAYCHANGE,
    WM_DRAWITEM, WM_DROPFILES, WM_ERASEBKGND, WM_GESTURE, WM_GETFONT, WM_GETMINMAXINFO,
    WM_GETOBJECT, WM_HSCROLL, WM_INITDIALOG, WM_INITMENUPOPUP, WM_KEYDOWN, WM_KEYFIRST, WM_KEYLAST,
    WM_KEYUP, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDBLCLK, WM_MBUTTONDOWN,
    WM_MBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEFIRST, WM_MOUSEHWHEEL, WM_MOUSELAST, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_MOVE, WM_NCCALCSIZE, WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDBLCLK,
    WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMBUTTONDBLCLK, WM_NCMBUTTONDOWN, WM_NCMBUTTONUP,
    WM_NCMOUSEMOVE, WM_NCPAINT, WM_NCRBUTTONDBLCLK, WM_NCRBUTTONDOWN, WM_NCRBUTTONUP, WM_NOTIFY,
    WM_PAINT, WM_RBUTTONDBLCLK, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETFONT,
    WM_SETREDRAW, WM_SETTEXT, WM_SHOWWINDOW, WM_SIZE, WM_STYLECHANGED, WM_SYSCHAR, WM_SYSCOMMAND,
    WM_SYSDEADCHAR, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WM_USER, WM_VSCROLL, WPARAM,
};

// Some constants which are calculated from other constants are not picked up by bindgen.
pub const TBM_GETPOS: u32 = WM_USER;
pub const TBM_SETTIC: u32 = WM_USER + 4;
pub const TBM_SETPOS: u32 = WM_USER + 5;
pub const TBM_SETRANGE: u32 = WM_USER + 6;
pub const TBM_SETSEL: u32 = WM_USER + 10;

// Some constants are different in SWELL. Search for "these differ" in SWELL source code for
// explanation.
#[cfg(target_family = "unix")]
pub use crate::bindings::root::{
    // SWP
    SWP_FRAMECHANGED,
    SWP_NOACTIVATE,
    SWP_NOCOPYBITS,
    SWP_NOMOVE,
    SWP_NOSIZE,
    SWP_NOZORDER,
    SWP_SHOWWINDOW,
    // SW
    SW_HIDE,
    SW_NORMAL,
    SW_RESTORE,
    SW_SHOW,
    SW_SHOWDEFAULT,
    SW_SHOWMAXIMIZED,
    SW_SHOWMINIMIZED,
    SW_SHOWNA,
    SW_SHOWNOACTIVATE,
    SW_SHOWNORMAL,
};

#[cfg(target_family = "windows")]
mod windows_constants {
    // SW
    pub const SW_HIDE: i32 = 0;
    pub const SW_NORMAL: i32 = 1;
    pub const SW_RESTORE: i32 = 9;
    pub const SW_SHOW: i32 = 5;
    pub const SW_SHOWDEFAULT: i32 = 10;
    pub const SW_SHOWMAXIMIZED: i32 = 3;
    pub const SW_SHOWMINIMIZED: i32 = 2;
    pub const SW_SHOWNA: i32 = 8;
    pub const SW_SHOWNOACTIVATE: i32 = 4;
    pub const SW_SHOWNORMAL: i32 = 1;

    // SWP
    pub const SWP_FRAMECHANGED: i32 = 0x0020;
    pub const SWP_NOACTIVATE: i32 = 0x0010;
    pub const SWP_NOCOPYBITS: i32 = 0x0100;
    pub const SWP_NOMOVE: i32 = 0x0002;
    pub const SWP_NOSIZE: i32 = 0x0001;
    pub const SWP_NOZORDER: i32 = 0x0004;
    pub const SWP_SHOWWINDOW: i32 = 0x0040;
}

#[cfg(target_family = "windows")]
pub use windows_constants::*;

/// Function pointer type for hook commands.
pub type HookCommand = extern "C" fn(command_id: c_int, flag: c_int) -> bool;

/// Function pointer type for toggle actions.
pub type ToggleAction = extern "C" fn(command_id: c_int) -> c_int;

/// Function pointer type for hook post commands.
pub type HookPostCommand = extern "C" fn(command_id: c_int, flag: c_int);
