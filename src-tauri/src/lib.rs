// Tauri application entry: registers commands and manages scrcpy process state.
//
// Command surface (mirrored by src/backend.ts):
//   adb_available()                       → bool
//   list_devices()                        → Vec<Device>
//   launch_scrcpy(serial, presetId, args) → ()   (tracks child in AppState)
//   connect_wireless(ip)                  → ()
//   stop_scrcpy()                         → ()   (kill interlock)

mod adb;
mod error;
mod scrcpy;

use error::{AppError, AppResult};
use std::sync::Arc;
use tokio::sync::Mutex;

// Holds the live scrcpy child so it can be killed on app exit / explicit stop.
#[derive(Default)]
pub struct AppState {
    child: Arc<Mutex<Option<tokio::process::Child>>>,
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
    state: tauri::State<'_, AppState>,
) -> AppResult<()> {
    let (child, stderr_ring) = scrcpy::launch(&serial, &args).await?;

    // Give scrcpy a moment; if it died instantly, surface the stderr tail.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let mut guard = state.child.lock().await;
    // Replace any prior child (kill it first to avoid orphans).
    if let Some(old) = guard.take() {
        let _ = scrcpy::kill(old).await;
    }

    let mut child = child;
    if let Ok(Some(status)) = child.try_wait() {
        if !status.success() {
            let tail = stderr_ring.last().unwrap_or_else(|| "scrcpy 已退出".into());
            return Err(AppError::ScrcpyLaunchFailed(tail));
        }
    }
    *guard = Some(child);
    Ok(())
}

#[tauri::command]
async fn stop_scrcpy(state: tauri::State<'_, AppState>) -> AppResult<()> {
    let mut guard = state.child.lock().await;
    if let Some(child) = guard.take() {
        scrcpy::kill(child).await?;
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
    // adb connect exits 0 even on failure; inspect the message.
    if stdout.contains("connected") {
        Ok(())
    } else {
        Err(AppError::WirelessConnectFailed(stdout.trim().to_string()))
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            adb_available,
            list_devices,
            launch_scrcpy,
            stop_scrcpy,
            connect_wireless
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
