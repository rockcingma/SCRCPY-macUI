// Key injection via `adb shell input keyevent` (PRD §3.2, revised).
//
// Why adb and not AppleScript (the original D1 choice)?
//   The AppleScript path (`tell process "scrcpy" ... keystroke`) was proven
//   broken in testing: macOS `keystroke` always targets the FOCUSED app, and
//   scrcpy's shortcuts (Cmd+H Home, Cmd+S Recents, Cmd+P Lock) collide with
//   macOS system shortcuts (Hide, Save, Print). When the float panel has
//   focus, Cmd+H hid OUR OWN window. When scrcpy had focus, Cmd+H hid scrcpy.
//   Either way the keystroke route is unusable.
//
//   adb keyevent talks straight to the Android input system:
//     - No window focus dependency (can't mis-target the Mac UI).
//     - No macOS system-shortcut collision (it's an Android keycode, not a
//       Mac keystroke).
//     - No Accessibility permission needed (adb is a normal subprocess).
//   Latency is ~100-400ms per call. For occasional nav-button taps on a
//   personal tool, that's fine — and it actually works, which beats a
//   theoretically-faster path that doesn't.
//
// Trust boundary: KeyAction is a fixed enum (deserialized from the frontend),
// and the serial is validated against a whitelist before reaching argv. No
// user-controlled string ever enters a shell — every adb call uses argv, not
// `sh -c`.

use crate::error::{AppError, AppResult};
use std::path::Path;

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

/// How an action maps onto adb. KeyEvent and Shell are dispatched here;
/// Special actions (screenshot/close/rotate) are handled by the caller
/// because they need extra state (file paths, process handles, multi-step
/// rotation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdbCommand {
    /// `adb -s <serial> shell input keyevent <code>`
    KeyEvent(u16),
    /// `adb -s <serial> shell <args...>` — for commands that aren't keyevents.
    Shell(&'static [&'static str]),
    /// Handled out-of-band by the dispatcher (lib.rs).
    Special(&'static str),
}

impl KeyAction {
    /// Maps a button to its adb realization. Android keycodes from
    /// `android.view.KeyEvent`.
    pub fn adb_command(self) -> AdbCommand {
        match self {
            KeyAction::Home => AdbCommand::KeyEvent(3), // KEYCODE_HOME
            KeyAction::Back => AdbCommand::KeyEvent(4), // KEYCODE_BACK
            KeyAction::Recents => AdbCommand::KeyEvent(187), // KEYCODE_APP_SWITCH
            KeyAction::Lock => AdbCommand::KeyEvent(26), // KEYCODE_POWER
            KeyAction::VolumeUp => AdbCommand::KeyEvent(24), // KEYCODE_VOLUME_UP
            KeyAction::VolumeDown => AdbCommand::KeyEvent(25), // KEYCODE_VOLUME_DOWN
            // `cmd statusbar expand-notifications` is more reliable than
            // KEYCODE_NOTIFICATION (83), which many ROMs ignore.
            KeyAction::Notifications => {
                AdbCommand::Shell(&["cmd", "statusbar", "expand-notifications"])
            }
            // Rotation needs read-modify-write of user_rotation — multi-step,
            // handled by rotate_screen() below.
            KeyAction::Rotate => AdbCommand::Special("rotate"),
            // Screenshot pulls a PNG to the desktop (lib.rs).
            KeyAction::Screenshot => AdbCommand::Special("screenshot"),
            // Close kills the scrcpy process (lib.rs).
            KeyAction::Close => AdbCommand::Special("close"),
        }
    }
}

/// Validate an adb serial (USB alphanumeric OR wireless IP[:port]). Delegates
/// to scrcpy::is_valid_serial so the two modules can't drift — both build argv
/// and need the same injection guard, and a wireless device's serial is an
/// IP:port that must be accepted for key injection too.
fn is_valid_serial(serial: &str) -> bool {
    crate::scrcpy::is_valid_serial(serial)
}

/// Build the full adb argv for a keyevent. Pure — unit-tested without adb.
///
///   build_keyevent_args("R5CX21RJ6MX", 3)
///     => ["-s", "R5CX21RJ6MX", "shell", "input", "keyevent", "3"]
pub fn build_keyevent_args(serial: &str, code: u16) -> Vec<String> {
    vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "input".into(),
        "keyevent".into(),
        code.to_string(),
    ]
}

/// Build the full adb argv for a shell command.
///
///   build_shell_args("R5CX21RJ6MX", &["cmd","statusbar","expand-notifications"])
///     => ["-s","R5CX21RJ6MX","shell","cmd","statusbar","expand-notifications"]
pub fn build_shell_args(serial: &str, cmd: &[&str]) -> Vec<String> {
    let mut args = vec!["-s".into(), serial.into(), "shell".into()];
    args.extend(cmd.iter().map(|s| s.to_string()));
    args
}

/// Execute a keyevent/shell action against the device. Special actions
/// return an error so the caller is forced to branch before calling here.
pub async fn inject(action: KeyAction, serial: &str, adb_path: &Path) -> AppResult<()> {
    if !is_valid_serial(serial) {
        return Err(AppError::KeyInjectFailed(format!("非法设备序列号: {serial}")));
    }
    let args = match action.adb_command() {
        AdbCommand::KeyEvent(code) => build_keyevent_args(serial, code),
        AdbCommand::Shell(cmd) => build_shell_args(serial, cmd),
        AdbCommand::Special(tag) => {
            return Err(AppError::KeyInjectFailed(format!(
                "special action '{tag}' must be handled by the dispatcher"
            )));
        }
    };
    run_adb(adb_path, &args).await
}

/// Rotate the screen one quarter turn. Disables auto-rotate first (otherwise
/// the sensor snaps it back), then advances user_rotation (0→1→2→3→0).
pub async fn rotate_screen(serial: &str, adb_path: &Path) -> AppResult<()> {
    if !is_valid_serial(serial) {
        return Err(AppError::KeyInjectFailed(format!("非法设备序列号: {serial}")));
    }
    // Pin auto-rotate off so the manual rotation sticks.
    run_adb(
        adb_path,
        &build_shell_args(serial, &["settings", "put", "system", "accelerometer_rotation", "0"]),
    )
    .await?;

    // Read current rotation (0-3), default 0 on any parse failure.
    let out = tokio::process::Command::new(adb_path)
        .args(build_shell_args(serial, &["settings", "get", "system", "user_rotation"]))
        .output()
        .await
        .map_err(|e| AppError::KeyInjectFailed(format!("adb spawn: {e}")))?;
    let current: u8 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap_or(0);
    let next = (current + 1) % 4;

    run_adb(
        adb_path,
        &build_shell_args(serial, &["settings", "put", "system", "user_rotation", &next.to_string()]),
    )
    .await
}

async fn run_adb(adb_path: &Path, args: &[String]) -> AppResult<()> {
    let out = tokio::process::Command::new(adb_path)
        .args(args)
        .output()
        .await
        .map_err(|e| AppError::KeyInjectFailed(format!("adb spawn: {e}")))?;
    if !out.status.success() {
        return Err(AppError::KeyInjectFailed(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_actions_map_to_keyevents() {
        assert_eq!(KeyAction::Home.adb_command(), AdbCommand::KeyEvent(3));
        assert_eq!(KeyAction::Back.adb_command(), AdbCommand::KeyEvent(4));
        assert_eq!(KeyAction::Recents.adb_command(), AdbCommand::KeyEvent(187));
        assert_eq!(KeyAction::Lock.adb_command(), AdbCommand::KeyEvent(26));
        assert_eq!(KeyAction::VolumeUp.adb_command(), AdbCommand::KeyEvent(24));
        assert_eq!(KeyAction::VolumeDown.adb_command(), AdbCommand::KeyEvent(25));
    }

    #[test]
    fn notifications_maps_to_statusbar_shell() {
        assert_eq!(
            KeyAction::Notifications.adb_command(),
            AdbCommand::Shell(&["cmd", "statusbar", "expand-notifications"])
        );
    }

    #[test]
    fn special_actions_are_marked_special() {
        assert_eq!(KeyAction::Rotate.adb_command(), AdbCommand::Special("rotate"));
        assert_eq!(KeyAction::Screenshot.adb_command(), AdbCommand::Special("screenshot"));
        assert_eq!(KeyAction::Close.adb_command(), AdbCommand::Special("close"));
    }

    #[test]
    fn keyevent_args_have_serial_and_code() {
        let args = build_keyevent_args("R5CX21RJ6MX", 3);
        assert_eq!(
            args,
            vec!["-s", "R5CX21RJ6MX", "shell", "input", "keyevent", "3"]
        );
    }

    #[test]
    fn keyevent_uses_correct_code_for_home() {
        // The exact bug that hid the Mac window: Home must be Android
        // KEYCODE_HOME (3), NOT a macOS Cmd+H keystroke.
        let AdbCommand::KeyEvent(code) = KeyAction::Home.adb_command() else {
            panic!("Home should be a keyevent");
        };
        let args = build_keyevent_args("ABCD1234", code);
        assert_eq!(args.last().unwrap(), "3");
        // Crucially: no "keystroke", no "command down", no osascript.
        assert!(!args.iter().any(|a| a.contains("keystroke")));
    }

    #[test]
    fn shell_args_prepend_serial_and_shell() {
        let args = build_shell_args("ABCD1234", &["cmd", "statusbar", "expand-notifications"]);
        assert_eq!(
            args,
            vec!["-s", "ABCD1234", "shell", "cmd", "statusbar", "expand-notifications"]
        );
    }

    #[test]
    fn inject_rejects_invalid_serial() {
        let adb = Path::new("/usr/bin/false");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(inject(KeyAction::Home, "bad; rm -rf /", adb));
        assert!(matches!(err, Err(AppError::KeyInjectFailed(_))));
    }

    #[test]
    fn inject_rejects_special_actions() {
        let adb = Path::new("/usr/bin/true");
        let rt = tokio::runtime::Runtime::new().unwrap();
        // Valid serial, but Close is special → must be rejected here.
        let err = rt.block_on(inject(KeyAction::Close, "R5CX21RJ6MX", adb));
        assert!(matches!(err, Err(AppError::KeyInjectFailed(_))));
    }

    #[test]
    fn key_action_deserialises_from_snake_case() {
        let action: KeyAction = serde_json::from_str("\"volume_up\"").unwrap();
        assert_eq!(action, KeyAction::VolumeUp);
    }

    #[test]
    fn unknown_key_action_deserialise_fails() {
        let result: Result<KeyAction, _> = serde_json::from_str("\"sudo_rm_rf\"");
        assert!(result.is_err());
    }
}
