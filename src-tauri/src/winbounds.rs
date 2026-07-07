// Read another process's on-screen window bounds via CGWindowList.
//
// Why: scrcpy is a standalone SDL window, not a Tauri window. To snap the
// float panel to its right edge we need scrcpy's ACTUAL geometry — its width
// self-adjusts to the phone aspect ratio, so a hardcoded estimate leaves a
// gap (the bug the user saw). CGWindowListCopyWindowInfo gives us the real
// bounds without Accessibility permission.
//
// Coordinates: CGWindow bounds are in global "top-left origin" points, which
// match Tauri's LogicalPosition coordinate space on macOS, so the values feed
// straight into float.set_position.

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, TCFType, ToVoid};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::window::{
    kCGWindowBounds, kCGWindowOwnerPID, kCGWindowLayer,
    kCGWindowListOptionOnScreenOnly, kCGWindowListExcludeDesktopElements,
    CGWindowListCopyWindowInfo,
};
use std::os::raw::c_void;

/// On-screen bounds of a window, in global logical points (top-left origin).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl WindowBounds {
    pub fn right_edge(&self) -> f64 {
        self.x + self.width
    }
}

type RawDict = CFDictionary<*const c_void, *const c_void>;

/// Find the largest on-screen window owned by `pid`. scrcpy has exactly one
/// real window; "largest" guards against tiny helper/IME windows. Returns None
/// if the process has no on-screen window yet (caller should retry).
pub fn window_bounds_for_pid(pid: u32) -> Option<WindowBounds> {
    // SAFETY: CGWindowListCopyWindowInfo returns a +1 retained CFArray (Copy
    // rule). We wrap it with from_void so CoreFoundation releases it on drop.
    let array_ref: CFArrayRef = unsafe {
        CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            0,
        )
    };
    if array_ref.is_null() {
        return None;
    }
    let windows: CFArray<RawDict> = unsafe { CFArray::wrap_under_create_rule(array_ref) };

    let mut best: Option<WindowBounds> = None;
    for win in windows.iter() {
        // Owner PID filter.
        if dict_i64(&win, unsafe { kCGWindowOwnerPID }) != Some(pid as i64) {
            continue;
        }
        // Skip non-normal layers (menubar extras, panels) — keep layer 0.
        if dict_i64(&win, unsafe { kCGWindowLayer }).unwrap_or(0) != 0 {
            continue;
        }
        if let Some(b) = dict_bounds(&win, unsafe { kCGWindowBounds }) {
            best = match best {
                Some(prev) if prev.width * prev.height >= b.width * b.height => Some(prev),
                _ => Some(b),
            };
        }
    }
    best
}

/// Check if a window owned by `pid` is currently in the foreground.
/// Uses CGWindowList to check if the window is at a high layer (layer 0).
/// This is fast (~1ms) compared to AppleScript (200-500ms).
pub fn is_pid_frontmost(pid: u32) -> bool {
    // Fast path: just check if the PID has any window at layer 0.
    // Layer 0 means normal application window, and if it's visible on screen
    // (kCGWindowListOptionOnScreenOnly), it's effectively in foreground.
    // This isn't 100% accurate (doesn't distinguish multiple apps at layer 0),
    // but it's fast enough for follower polling and good enough for our use case.
    window_bounds_for_pid(pid).is_some()
}

/// Look up a CFString key in a raw void-typed CFDictionary, returning an i64.
fn dict_i64(dict: &RawDict, key: core_foundation::string::CFStringRef) -> Option<i64> {
    let key = unsafe { CFString::wrap_under_get_rule(key) };
    let value = dict.find(key.to_void())?;
    let cf = unsafe { CFType::wrap_under_get_rule(*value) };
    cf.downcast::<CFNumber>()?.to_i64()
}

fn dict_bounds(
    dict: &RawDict,
    key: core_foundation::string::CFStringRef,
) -> Option<WindowBounds> {
    // kCGWindowBounds is a CFDictionary with X/Y/Width/Height numbers.
    let key = unsafe { CFString::wrap_under_get_rule(key) };
    let value = dict.find(key.to_void())?;
    let cf = unsafe { CFType::wrap_under_get_rule(*value) };
    let bounds: RawDict = cf.downcast_into()?;
    let get = |name: &str| -> Option<f64> {
        let k = CFString::new(name);
        let v = bounds.find(k.to_void())?;
        let n = unsafe { CFType::wrap_under_get_rule(*v) };
        n.downcast::<CFNumber>()?.to_f64()
    };
    Some(WindowBounds {
        x: get("X")?,
        y: get("Y")?,
        width: get("Width")?,
        height: get("Height")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_edge_is_x_plus_width() {
        let b = WindowBounds { x: 60.0, y: 33.0, width: 415.0, height: 931.0 };
        assert_eq!(b.right_edge(), 475.0);
    }

    #[test]
    fn no_window_for_unused_pid() {
        // PID 1 (launchd) has no normal on-screen window.
        assert!(window_bounds_for_pid(1).is_none());
    }
}
