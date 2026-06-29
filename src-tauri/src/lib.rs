// Tauri application entry: registers commands and manages scrcpy process state.
//
// Command surface (mirrored by src/backend.ts):
//   adb_available()                       → bool
//   list_devices()                        → Vec<Device>
//   launch_scrcpy(serial, args)           → ()    (tracks child + emits events)
//   stop_scrcpy()                         → ()    (kill interlock)
//   connect_wireless(ip)                  → ()
//   send_key(action)                      → ()    (dispatches via keyinject)
//   take_screenshot(serial)               → String (saved path on disk)
//   accessibility_status()                → bool   (osascript reachable?)
//
// Events emitted on the global Tauri channel:
//   "scrcpy-started"  → float window should show
//   "scrcpy-stopped"  → float window should hide
//
// Lifecycle:
//   launch_scrcpy spawns scrcpy + a tokio watcher. The watcher awaits the
//   child's exit and emits "scrcpy-stopped" so the UI can react regardless
//   of how the process dies (clean exit, crash, external kill).

mod adb;
mod error;
mod keyinject;
mod scrcpy;

use error::{AppError, AppResult};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

// Holds the live scrcpy child so it can be killed on app exit / explicit stop.
#[derive(Default)]
pub struct AppState {
    child: Arc<Mutex<Option<tokio::process::Child>>>,
    // Last serial used to launch scrcpy. Needed for screenshot (adb pull).
    serial: Arc<Mutex<Option<String>>>,
}

#[tauri::command]
fn adb_available() -> bool {
    adb::adb_available()
}

#[tauri::command]
async fn list_devices() -> AppResult<Vec<adb::Device>> {
    adb::list_devices().await
}

#[tauri::command]
async fn launch_scrcpy(
    serial: String,
    args: Vec<String>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> AppResult<()> {
    let (mut child, stderr_ring) = scrcpy::launch(&serial, &args).await?;

    // Give scrcpy a moment; if it died instantly, surface the stderr tail.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    if let Ok(Some(status)) = child.try_wait() {
        if !status.success() {
            let tail = stderr_ring.last().unwrap_or_else(|| "scrcpy 已退出".into());
            return Err(AppError::ScrcpyLaunchFailed(tail));
        }
    }

    let mut guard = state.child.lock().await;
    // Replace any prior child (kill it first to avoid orphans).
    if let Some(old) = guard.take() {
        let _ = scrcpy::kill(old).await;
    }
    *state.serial.lock().await = Some(serial);
    *guard = Some(child);
    drop(guard);

    // Tell the UI scrcpy is live so the float panel can fade in.
    let _ = app.emit("scrcpy-started", ());

    // Spawn the watcher: when scrcpy exits (for any reason), tell the UI.
    spawn_lifecycle_watcher(app.clone(), state.child.clone());

    Ok(())
}

fn spawn_lifecycle_watcher(
    app: tauri::AppHandle,
    child_slot: Arc<Mutex<Option<tokio::process::Child>>>,
) {
    tokio::spawn(async move {
        // Poll the slot until the child exits or is replaced.
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let mut guard = child_slot.lock().await;
            let still_running = match guard.as_mut() {
                Some(c) => matches!(c.try_wait(), Ok(None)),
                None => {
                    // Slot empty — somebody else cleaned up. Exit watcher.
                    return;
                }
            };
            if !still_running {
                // Reap and clear the slot.
                if let Some(mut c) = guard.take() {
                    let _ = c.wait().await;
                }
                drop(guard);
                let _ = app.emit("scrcpy-stopped", ());
                return;
            }
        }
    });
}

#[tauri::command]
async fn stop_scrcpy(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> AppResult<()> {
    let mut guard = state.child.lock().await;
    if let Some(child) = guard.take() {
        scrcpy::kill(child).await?;
        drop(guard);
        let _ = app.emit("scrcpy-stopped", ());
    }
    Ok(())
}

#[tauri::command]
async fn connect_wireless(ip: String) -> AppResult<()> {
    if !scrcpy::is_valid_ip(&ip) {
        return Err(AppError::WirelessConnectFailed(format!("非法地址: {ip}")));
    }
    let adb = adb::detect_adb_path().ok_or(AppError::AdbNotFound)?;
    let target = if ip.contains(':') { ip.clone() } else { format!("{ip}:5555") };
    let out = tokio::process::Command::new(adb)
        .args(["connect", &target])
        .output()
        .await?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.contains("connected") {
        Ok(())
    } else {
        Err(AppError::WirelessConnectFailed(stdout.trim().to_string()))
    }
}

#[tauri::command]
async fn send_key(
    action: keyinject::KeyAction,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> AppResult<()> {
    use keyinject::KeyAction;
    // Special actions short-circuit before reaching osascript.
    match action {
        KeyAction::Close => stop_scrcpy(app, state).await,
        KeyAction::Screenshot => {
            let serial = state.serial.lock().await.clone()
                .ok_or(AppError::DeviceNotConnected)?;
            take_screenshot_inner(&serial).await.map(|_| ())
        }
        _ => keyinject::inject(action).await,
    }
}

#[tauri::command]
async fn accessibility_status() -> bool {
    // Probe by attempting a no-op keystroke. `keystroke ""` doesn't actually
    // type anything (System Events validates the input first), but it goes
    // through the same authorisation gate that real keystrokes do — so a
    // denied app returns -25211 / 1002 here.
    //
    // Previously this command only checked `name of current user`, which
    // requires Apple Events but NOT Accessibility — leading to false
    // positives that hid the authorization banner until the first real
    // injection failed.
    let out = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to keystroke \"\"")
        .output()
        .await;
    match out {
        Ok(o) if o.status.success() => true,
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            !matches!(
                keyinject::classify_osascript_stderr(&stderr),
                AppError::AccessibilityDenied
            )
        }
        Err(_) => false,
    }
}

/// Open macOS System Settings → Privacy → Accessibility.
///
/// Why this exists: WebView blocks `window.location.href = "x-apple.systempreferences:..."`
/// for security reasons. The frontend can't navigate to system URL schemes,
/// so we shell out to `open` here, which handles the scheme natively.
#[tauri::command]
async fn open_accessibility_settings() -> AppResult<()> {
    tokio::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn()
        .map_err(|e| AppError::Io(format!("open failed: {e}")))?;
    Ok(())
}

async fn take_screenshot_inner(serial: &str) -> AppResult<String> {
    let adb_path = adb::detect_adb_path().ok_or(AppError::AdbNotFound)?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let home = std::env::var("HOME").map_err(|e| AppError::Io(e.to_string()))?;
    let local_path = format!("{home}/Desktop/scrcpy-{serial}-{timestamp}.png");

    // screencap directly to host stdout, write atomically.
    let out = tokio::process::Command::new(&adb_path)
        .args(["-s", serial, "exec-out", "screencap", "-p"])
        .output()
        .await?;
    if !out.status.success() {
        return Err(AppError::Io(String::from_utf8_lossy(&out.stderr).into_owned()));
    }
    tokio::fs::write(&local_path, &out.stdout).await
        .map_err(|e| AppError::Io(e.to_string()))?;
    Ok(local_path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(AppState::default())
        .setup(|app| {
            // Hide the float window at startup; launch_scrcpy will show it.
            if let Some(float) = app.get_webview_window("float") {
                let _ = float.hide();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            adb_available,
            list_devices,
            launch_scrcpy,
            stop_scrcpy,
            connect_wireless,
            send_key,
            accessibility_status,
            open_accessibility_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
