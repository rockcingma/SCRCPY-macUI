// Key injection via AppleScript → scrcpy SDL window (PRD §3.2, D1).
//
// Why AppleScript and not `adb shell input keyevent`?
//   `adb shell input` cold-starts an Android JVM per call → 0.5–1s latency.
//   AppleScript dispatches a host-side keystroke into the focused scrcpy
//   window, which then forwards over scrcpy's persistent control socket at
//   ~50ms total. The user feels the click register.
//
// Trust boundary: every button maps to a fixed AppleScript template here.
// User input never enters the script body — only the action enum does.

use crate::error::{AppError, AppResult};

/// One of the ten supported floating-panel actions.
#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyAction {
    Home,
    Back,
    Recents,
    Lock,
    Screenshot,
    VolumeUp,
    VolumeDown,
    Notifications,
    Rotate,
    Close,
}

impl KeyAction {
    /// Maps a button to the scrcpy keyboard shortcut it triggers.
    ///
    /// Source: scrcpy 4.0 mac shortcuts (PRD §3.2 mapping table).
    /// First field is the literal key char, second is the modifier mask.
    /// `cmd_shift = true` means Cmd+Shift, otherwise Cmd alone.
    pub fn shortcut(self) -> Shortcut {
        match self {
            KeyAction::Home => Shortcut::cmd('h'),
            KeyAction::Back => Shortcut::cmd('b'),
            KeyAction::Recents => Shortcut::cmd('s'),
            KeyAction::Lock => Shortcut::cmd('p'),
            // Screenshot doesn't have a scrcpy shortcut — we'll handle it
            // out-of-band via `adb shell screencap` in lib.rs. Marker only.
            KeyAction::Screenshot => Shortcut::special("screenshot"),
            KeyAction::VolumeUp => Shortcut::cmd('+'),
            KeyAction::VolumeDown => Shortcut::cmd('-'),
            // scrcpy uses Cmd+N to expand notifications.
            KeyAction::Notifications => Shortcut::cmd('n'),
            KeyAction::Rotate => Shortcut::cmd_shift('r'),
            // Close is "kill scrcpy", handled by the Rust process layer.
            KeyAction::Close => Shortcut::special("close"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub key: char,
    pub cmd: bool,
    pub shift: bool,
    /// Set for non-keystroke actions (screenshot / close). The lib.rs
    /// dispatcher branches on `special` before reaching osascript.
    pub special: Option<&'static str>,
}

impl Shortcut {
    fn cmd(key: char) -> Self {
        Self { key, cmd: true, shift: false, special: None }
    }
    fn cmd_shift(key: char) -> Self {
        Self { key, cmd: true, shift: true, special: None }
    }
    fn special(tag: &'static str) -> Self {
        Self { key: '\0', cmd: false, shift: false, special: Some(tag) }
    }
}

/// Build the AppleScript that sends a keystroke to scrcpy.
///
/// scrcpy is NOT a macOS .app bundle — it's a plain SDL binary. Earlier
/// we tried `set frontmost of process "scrcpy" to true` before keystroke,
/// but that triggers a Cmd+H conflict: Cmd+H is both scrcpy's "Home" AND
/// macOS's system "Hide Application" shortcut. When scrcpy is frontmost,
/// both fire — the phone goes home (scrcpy handled it) and the scrcpy
/// window hides (macOS handled it).
///
/// Fix: send the keystroke directly to the scrcpy process without
/// activating it first. System Events allows this for background processes
/// if Accessibility is granted. The trade-off: if another app is focused
/// and steals the keystroke before it reaches scrcpy, this fails silently.
/// In practice scrcpy is usually visible when the float panel is used, so
/// the keystroke reaches it.
///
/// Output:
///   tell application "System Events"
///     tell process "scrcpy"
///       keystroke "h" using {command down}
///     end tell
///   end tell
pub fn build_applescript(shortcut: &Shortcut) -> AppResult<String> {
    if shortcut.special.is_some() {
        return Err(AppError::KeyInjectFailed(
            "special action has no AppleScript".into(),
        ));
    }
    if !shortcut.key.is_ascii() || shortcut.key.is_control() {
        return Err(AppError::KeyInjectFailed(format!(
            "non-ascii key: {:?}",
            shortcut.key
        )));
    }
    let mods = match (shortcut.cmd, shortcut.shift) {
        (true, true) => " using {command down, shift down}",
        (true, false) => " using {command down}",
        (false, true) => " using {shift down}",
        (false, false) => "",
    };
    Ok(format!(
        "tell application \"System Events\"\n\
         \ttell process \"scrcpy\"\n\
         \t\tkeystroke \"{key}\"{mods}\n\
         \tend tell\n\
         end tell",
        key = shortcut.key,
        mods = mods,
    ))
}

/// Classify an osascript stderr line. macOS surfaces "not authorised for
/// Accessibility" under several error codes and locales:
///   -25211 (English: "not allowed assistive access")
///    1002  (zh-CN: "osascript 不允许发送按键")
///    -1719 (some older builds)
/// Treat any of those as AccessibilityDenied so the UI can route the user
/// to System Settings (PRD §3.3 last row).
pub fn classify_osascript_stderr(stderr: &str) -> AppError {
    let lower = stderr.to_lowercase();
    let denied = stderr.contains("-25211")
        || stderr.contains("(1002)")
        || stderr.contains("-1719")
        || lower.contains("not allowed assistive")
        || lower.contains("not allowed to send keystrokes")
        || stderr.contains("不允许发送按键")
        || stderr.contains("不被允许")
        || stderr.contains("辅助功能");
    if denied {
        AppError::AccessibilityDenied
    } else {
        AppError::KeyInjectFailed(stderr.trim().to_string())
    }
}

/// Execute the action against a running scrcpy window.
///
/// Screenshot and close are special — they bypass osascript entirely.
/// Everything else goes through `osascript -e <script>` which emits the
/// keystroke into the scrcpy SDL window.
pub async fn inject(action: KeyAction) -> AppResult<()> {
    let shortcut = action.shortcut();
    if let Some(tag) = shortcut.special {
        // Special actions are dispatched by the caller (lib.rs). Returning
        // an error keeps the contract explicit: callers MUST branch first.
        return Err(AppError::KeyInjectFailed(format!(
            "special action '{tag}' must be handled by the dispatcher"
        )));
    }
    let script = build_applescript(&shortcut)?;
    let out = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .await
        .map_err(|e| AppError::KeyInjectFailed(format!("osascript spawn: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(classify_osascript_stderr(&stderr));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_has_a_shortcut() {
        // Smoke-check: KeyAction → Shortcut never panics.
        for action in [
            KeyAction::Home, KeyAction::Back, KeyAction::Recents,
            KeyAction::Lock, KeyAction::Screenshot, KeyAction::VolumeUp,
            KeyAction::VolumeDown, KeyAction::Notifications,
            KeyAction::Rotate, KeyAction::Close,
        ] {
            let _ = action.shortcut();
        }
    }

    #[test]
    fn keystroke_actions_build_clean_applescript() {
        let script = build_applescript(&KeyAction::Home.shortcut()).unwrap();
        // No activation — send keystroke directly to the background process.
        assert!(script.contains("tell process \"scrcpy\""));
        assert!(script.contains("keystroke \"h\""));
        assert!(script.contains("using {command down}"));
        assert!(!script.contains("set frontmost")); // Should NOT activate.
    }

    #[test]
    fn cmd_shift_renders_both_modifiers() {
        let script = build_applescript(&KeyAction::Rotate.shortcut()).unwrap();
        assert!(script.contains("keystroke \"r\""));
        assert!(script.contains("{command down, shift down}"));
    }

    #[test]
    fn screenshot_action_has_no_applescript() {
        let s = KeyAction::Screenshot.shortcut();
        assert!(s.special.is_some());
        assert!(build_applescript(&s).is_err());
    }

    #[test]
    fn close_action_has_no_applescript() {
        let s = KeyAction::Close.shortcut();
        assert!(s.special.is_some());
        assert!(build_applescript(&s).is_err());
    }

    #[test]
    fn classify_recognises_accessibility_error() {
        let stderr = "execution error: System Events got an error: osascript is not allowed assistive access. (-25211)";
        assert!(matches!(
            classify_osascript_stderr(stderr),
            AppError::AccessibilityDenied
        ));
    }

    #[test]
    fn classify_recognises_chinese_accessibility_error() {
        // Real macOS 14.x zh-CN output observed on the dev machine.
        let stderr = "execution error: \"System Events\"遇到一个错误：\"osascript\"不允许发送按键。 (1002)";
        assert!(matches!(
            classify_osascript_stderr(stderr),
            AppError::AccessibilityDenied
        ));
    }

    #[test]
    fn classify_recognises_legacy_error_codes() {
        for stderr in [
            "execution error: not allowed assistive (-25211)",
            "execution error: 不被允许 (-1719)",
        ] {
            assert!(matches!(
                classify_osascript_stderr(stderr),
                AppError::AccessibilityDenied
            ), "should classify as denied: {stderr}");
        }
    }

    #[test]
    fn classify_falls_back_to_generic_failure() {
        let err = classify_osascript_stderr("execution error: something else (-1)");
        match err {
            AppError::KeyInjectFailed(msg) => assert!(msg.contains("something else")),
            other => panic!("expected KeyInjectFailed, got {other:?}"),
        }
    }

    #[test]
    fn key_action_deserialises_from_snake_case() {
        let action: KeyAction = serde_json::from_str("\"volume_up\"").unwrap();
        assert_eq!(action, KeyAction::VolumeUp);
    }

    #[test]
    fn unknown_key_action_deserialise_fails() {
        // Defends the trust boundary: the frontend can only send known
        // actions. Garbage gets rejected at the deserialisation seam.
        let result: Result<KeyAction, _> = serde_json::from_str("\"sudo_rm_rf\"");
        assert!(result.is_err());
    }
}
