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
mod winbounds;

use error::{AppError, AppResult};
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

// Holds the live scrcpy child so it can be killed on app exit / explicit stop.
pub struct AppState {
    child: Arc<Mutex<Option<tokio::process::Child>>>,
    // Last serial used to launch scrcpy. Needed for screenshot (adb pull).
    serial: Arc<Mutex<Option<String>>>,
    // The preset args of the current mirror session (WITHOUT --record), so a
    // recording toggle can relaunch with the same quality and then return to
    // plain mirroring without interrupting the user's chosen settings.
    base_args: Arc<Mutex<Vec<String>>>,
    // Some(path) while recording to that .mp4, None when not recording.
    recording: Arc<Mutex<Option<String>>>,
    // True while scrcpy is launched with --turn-screen-off (phone screen dark
    // but mirroring continues). Stays through recording toggles because they
    // rebuild argv from base_args + every active runtime flag.
    screen_off: Arc<Mutex<bool>>,
    // Audio routing. true (default) = scrcpy captures audio and plays it on
    // the Mac. false = scrcpy launched with --no-audio, so the phone keeps
    // playing through its own speakers. Like screen_off, this is a launch-time
    // flag, so flipping it goes through spawn_and_swap.
    host_audio: Arc<Mutex<bool>>,
    // User-chosen directory for screen recordings. None = default to
    // ~/Desktop, matching the previous behaviour. Stored in the Tauri store
    // by the frontend; we just consume the latest value here.
    record_dir: Arc<Mutex<Option<String>>>,
    // Handle to the float-follow watcher (polls scrcpy's window bounds and
    // snaps the float panel to its right edge so dragging scrcpy drags the
    // float with it). Aborted whenever scrcpy is replaced or stops.
    follower: Arc<Mutex<Option<tokio::task::AbortHandle>>>,
    // Always-on-top toggle. true = scrcpy window stays on top of all others.
    // false (default) = normal window behavior. Like other runtime flags, this
    // requires relaunching scrcpy with --always-on-top flag.
    always_on_top: Arc<Mutex<bool>>,
    // Original screen_off_timeout value (in ms) from the device, stored when
    // screen_off is enabled so it can be restored when disabled. None = not
    // captured yet or screen_off is inactive.
    original_screen_timeout: Arc<Mutex<Option<String>>>,
    // Last known position of scrcpy window (x, y in logical coordinates).
    // Used to restore position when relaunching scrcpy (e.g., during toggle_*).
    // None = use default position from layout module.
    last_window_position: Arc<Mutex<Option<(i32, i32)>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            serial: Arc::new(Mutex::new(None)),
            base_args: Arc::new(Mutex::new(Vec::new())),
            recording: Arc::new(Mutex::new(None)),
            screen_off: Arc::new(Mutex::new(false)),
            // host_audio defaults to true: scrcpy already mirrors audio out
            // of the box, so this matches the user's prior behaviour.
            host_audio: Arc::new(Mutex::new(true)),
            record_dir: Arc::new(Mutex::new(None)),
            follower: Arc::new(Mutex::new(None)),
            always_on_top: Arc::new(Mutex::new(false)),
            original_screen_timeout: Arc::new(Mutex::new(None)),
            last_window_position: Arc::new(Mutex::new(None)),
        }
    }
}

/// Recording state returned to the frontend (mirrors TS RecordingState).
/// camelCase to match the TS interface field names.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingState {
    recording: bool,
    saved_path: Option<String>,
}

/// Screen-off state returned to the frontend (mirrors TS ScreenOffState).
/// Just the new boolean — the toggle is symmetric, no extra payload needed.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenOffState {
    screen_off: bool,
}

/// Audio-host state returned to the frontend (mirrors TS AudioHostState).
/// true = Mac plays the device's audio; false = device speakers do.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioHostState {
    host_audio: bool,
}

/// Always-on-top state returned to the frontend (mirrors TS AlwaysOnTopState).
/// true = scrcpy window stays on top; false = normal window behavior.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlwaysOnTopState {
    always_on_top: bool,
}

/// Result of set_record_dir: tells the UI whether the chosen path was usable
/// and what the effective directory is (the chosen one, or the default if we
/// fell back). camelCase for TS parity.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordDirState {
    /// The directory now in effect — never an empty string.
    effective: String,
    /// True if the caller's chosen path was accepted as-is.
    accepted: bool,
    /// User-facing reason when accepted=false (e.g. "目录不可写").
    message: Option<String>,
}

// ── Window layout (PRD: mirror + float on the same screen, float snapped to
// the right of the mirror, positions relatively fixed) ───────────────────────
//
// Deterministic placement (simplified approach): we pin scrcpy's window to a
// fixed offset inside the user's monitor and place the float panel just to its
// right. scrcpy honors --window-x exactly; its width self-adjusts to the phone
// aspect ratio (~415px tall-portrait at this height), so we reserve a fixed
// width estimate and snap the float to it. Small (<20px) gaps are acceptable
// per the simplified scope; we don't read scrcpy's live geometry.
mod layout {
    pub const MIRROR_X: f64 = 60.0; // left inset of the scrcpy window
    pub const MIRROR_Y: f64 = 40.0; // top inset (below the menu bar)
    pub const MIRROR_H: f64 = 900.0; // requested scrcpy window height
    pub const MIRROR_W_EST: f64 = 430.0; // reserved width for a portrait phone
    pub const GAP: f64 = 8.0; // gap between mirror and float panel

    /// scrcpy CLI window-geometry args, anchored at the monitor origin (ox,oy).
    pub fn scrcpy_window_args(ox: f64, oy: f64) -> Vec<String> {
        vec![
            format!("--window-x={}", (ox + MIRROR_X) as i64),
            format!("--window-y={}", (oy + MIRROR_Y) as i64),
            format!("--window-height={}", MIRROR_H as i64),
        ]
    }

    /// Logical (x,y) for the float window: snapped to the mirror's right edge.
    pub fn float_position(ox: f64, oy: f64) -> (f64, f64) {
        (ox + MIRROR_X + MIRROR_W_EST + GAP, oy + MIRROR_Y)
    }
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
    // Fresh launch from a preset → not recording; remember the base args.
    // base_args is the user's quality preset WITHOUT window-geometry flags, so
    // a recording relaunch reuses the same quality. Window geometry is appended
    // fresh on every spawn (it depends on the current monitor).
    *state.serial.lock().await = Some(serial.clone());
    *state.base_args.lock().await = args.clone();
    *state.recording.lock().await = None;
    *state.screen_off.lock().await = false;
    // Reset audio routing to host (scrcpy's default). The audio toggle starts
    // fresh on every launch — like screen-off and recording.
    *state.host_audio.lock().await = true;

    spawn_and_swap(&serial, &args, &app, &state).await?;

    // Watcher: when scrcpy exits (for any reason), tell the UI.
    // Lives for the app's lifetime — re-uses across toggle_recording relaunches
    // (those swap child under the same lock, so the watcher never sees a gap).
    spawn_lifecycle_watcher(app.clone(), state.child.clone(), state.follower.clone());
    Ok(())
}

/// The logical origin (top-left) of the monitor the main window is on. Falls
/// back to (0,0) if it can't be resolved.
fn main_monitor_origin(app: &tauri::AppHandle) -> (f64, f64) {
    if let Some(main) = app.get_webview_window("main") {
        if let Ok(Some(monitor)) = main.current_monitor() {
            let scale = monitor.scale_factor();
            let pos = monitor.position().to_logical::<f64>(scale);
            return (pos.x, pos.y);
        }
    }
    (0.0, 0.0)
}

/// Spawn scrcpy with `args`, verify it didn't instantly die, and swap it into
/// the child slot (killing any prior child first to avoid orphans). Shared by
/// launch_scrcpy and toggle_recording.
///
/// Appends window-geometry flags so the mirror lands at a fixed spot on the
/// user's monitor, then snaps the float panel to its right edge.
async fn spawn_and_swap(
    serial: &str,
    args: &[String],
    app: &tauri::AppHandle,
    state: &tauri::State<'_, AppState>,
) -> AppResult<()> {
    // Pin scrcpy's window to the main window's monitor. If we have a saved
    // position from a previous launch, use it; otherwise fall back to the
    // default layout position.
    let (ox, oy) = main_monitor_origin(app);
    let mut full_args = args.to_vec();

    let saved_pos = state.last_window_position.lock().await.clone();
    if let Some((saved_x, saved_y)) = saved_pos {
        // Use saved position (absolute screen coordinates).
        full_args.push(format!("--window-x={}", saved_x));
        full_args.push(format!("--window-y={}", saved_y));
        full_args.push(format!("--window-height={}", layout::MIRROR_H as i64));
    } else {
        // Use default layout position.
        full_args.extend(layout::scrcpy_window_args(ox, oy));
    }

    let (mut child, stderr_ring) = scrcpy::launch(serial, &full_args).await?;
    let scrcpy_pid = child.id();

    // Give scrcpy a moment; if it died instantly, surface the stderr tail.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    if let Ok(Some(status)) = child.try_wait() {
        if !status.success() {
            let tail = stderr_ring.last().unwrap_or_else(|| "scrcpy 已退出".into());
            return Err(AppError::ScrcpyLaunchFailed(tail));
        }
    }

    let mut guard = state.child.lock().await;
    // Kill+replace under the lock so the lifecycle watcher never observes the
    // gap (it would otherwise emit a spurious scrcpy-stopped mid-relaunch).
    if let Some(old) = guard.take() {
        let _ = scrcpy::kill(old).await;
    }
    *guard = Some(child);
    drop(guard);

    // Snap the float panel to scrcpy's ACTUAL right edge. scrcpy's width
    // self-adjusts to the phone aspect ratio, so we read its real window
    // bounds (CGWindowList) instead of guessing — that guess left a big gap.
    // The window takes a moment to map; retry briefly.
    let bounds = match scrcpy_pid {
        Some(pid) => read_scrcpy_bounds_retry(pid).await,
        None => None,
    };

    // Position float window. When we have saved position, bounds should be
    // available since scrcpy will launch at that saved position. If bounds
    // reading failed, use saved position + estimated width to place float.
    let saved_pos = state.last_window_position.lock().await.clone();
    if bounds.is_some() {
        show_float_window(app, bounds);
    } else if let Some((saved_x, saved_y)) = saved_pos {
        // Fallback with saved position: estimate float position based on
        // saved scrcpy position + typical portrait width.
        show_float_window_at(app, saved_x, saved_y);
    } else {
        // Ultimate fallback: use monitor origin for default layout.
        show_float_window_fallback(app, ox, oy);
    }

    // Start (or restart) the follower so dragging scrcpy drags the float panel
    // with it. spawn_and_swap is called for fresh launches AND recording
    // toggles — both invalidate the previous follower (new PID), so we always
    // abort the old one before starting the new one.
    abort_follower(state).await;
    if let Some(pid) = scrcpy_pid {
        start_follower(app, state, pid).await;
    }
    Ok(())
}

/// Poll CGWindowList up to ~1.5s for scrcpy's window to appear and return its
/// bounds. Runs the blocking CG calls on a blocking thread.
fn read_scrcpy_bounds_retry(
    pid: u32,
) -> impl std::future::Future<Output = Option<winbounds::WindowBounds>> {
    async move {
        for _ in 0..15 {
            if let Some(b) =
                tokio::task::spawn_blocking(move || winbounds::window_bounds_for_pid(pid))
                    .await
                    .ok()
                    .flatten()
            {
                return Some(b);
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        None
    }
}

/// Show the float panel and snap it to the right edge of scrcpy's real window
/// (`bounds`). Assumes bounds are available from read_scrcpy_bounds_retry.
fn show_float_window(app: &tauri::AppHandle, bounds: Option<winbounds::WindowBounds>) {
    use tauri::LogicalPosition;
    let Some(float) = app.get_webview_window("float") else {
        return;
    };
    let _ = float.show();
    // Re-assert always-on-top so a freshly-spawned scrcpy window can't cover it.
    let _ = float.set_always_on_top(false);
    let _ = float.set_always_on_top(true);

    if let Some(b) = bounds {
        // Snap to scrcpy's real right edge + gap, aligned to its top.
        let _ = float.set_position(LogicalPosition::new(
            b.right_edge() + layout::GAP,
            b.y,
        ));
    }
}

/// Fallback float window positioning using layout estimates when scrcpy bounds
/// couldn't be read (e.g., launch failure or no saved position).
fn show_float_window_fallback(app: &tauri::AppHandle, ox: f64, oy: f64) {
    use tauri::LogicalPosition;
    let Some(float) = app.get_webview_window("float") else {
        return;
    };
    let _ = float.show();
    let _ = float.set_always_on_top(false);
    let _ = float.set_always_on_top(true);

    let (x, y) = layout::float_position(ox, oy);
    let _ = float.set_position(LogicalPosition::new(x, y));
}

/// Position float window based on saved scrcpy position when bounds reading
/// failed. Uses estimated portrait phone width to place float at right edge.
fn show_float_window_at(app: &tauri::AppHandle, scrcpy_x: i32, scrcpy_y: i32) {
    use tauri::LogicalPosition;
    let Some(float) = app.get_webview_window("float") else {
        return;
    };
    let _ = float.show();
    let _ = float.set_always_on_top(false);
    let _ = float.set_always_on_top(true);

    // Estimate: scrcpy_x + typical portrait width (~430px) + gap.
    let float_x = scrcpy_x as f64 + layout::MIRROR_W_EST + layout::GAP;
    let float_y = scrcpy_y as f64;
    let _ = float.set_position(LogicalPosition::new(float_x, float_y));
}

/// Hide the float panel (scrcpy stopped).
fn hide_float_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(float) = app.get_webview_window("float") {
        let _ = float.hide();
    }
}

/// Spawn the float-follower task. It polls scrcpy's window bounds every
/// FOLLOW_INTERVAL_MS and re-snaps the float panel whenever bounds change, so
/// dragging scrcpy drags the float with it. Exits on its own once the PID
/// disappears (bounds == None on consecutive polls), or when aborted by
/// `abort_follower` (recording relaunch, stop_scrcpy, app exit).
const FOLLOW_INTERVAL_MS: u64 = 100;
const FOLLOW_MISS_BUDGET: u8 = 5; // ~500ms of "no window" before we give up

async fn start_follower(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, AppState>,
    pid: u32,
) {
    let app = app.clone();
    let last_pos = state.last_window_position.clone();
    let handle = tokio::spawn(async move {
        let mut last: Option<winbounds::WindowBounds> = None;
        let mut misses: u8 = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(FOLLOW_INTERVAL_MS)).await;
            let bounds = tokio::task::spawn_blocking(move || {
                winbounds::window_bounds_for_pid(pid)
            })
            .await
            .ok()
            .flatten();

            match bounds {
                Some(b) => {
                    misses = 0;
                    if last != Some(b) {
                        reposition_float_window(&app, b);
                        // Save window position for next relaunch (convert f64 to i32).
                        *last_pos.lock().await = Some((b.x as i32, b.y as i32));
                        last = Some(b);
                    }
                }
                None => {
                    misses = misses.saturating_add(1);
                    if misses >= FOLLOW_MISS_BUDGET {
                        // scrcpy's window vanished — the lifecycle watcher
                        // will hide the float; we just stop polling.
                        return;
                    }
                }
            }
        }
    });
    *state.follower.lock().await = Some(handle.abort_handle());
}

/// Abort the current follower (if any). Called whenever scrcpy is replaced or
/// stops, so we never have two followers updating the float in parallel.
async fn abort_follower(state: &tauri::State<'_, AppState>) {
    if let Some(h) = state.follower.lock().await.take() {
        h.abort();
    }
}

/// Move the float panel to align with scrcpy's right edge. Same math as the
/// happy-path branch of `show_float_window`, factored out so the follower can
/// reuse it without re-showing or re-asserting always-on-top.
fn reposition_float_window(app: &tauri::AppHandle, bounds: winbounds::WindowBounds) {
    use tauri::LogicalPosition;
    if let Some(float) = app.get_webview_window("float") {
        let _ = float.set_position(LogicalPosition::new(
            bounds.right_edge() + layout::GAP,
            bounds.y,
        ));
    }
}

/// Sync float window's z-order (foreground/background) with scrcpy's state.
/// When scrcpy is frontmost, float stays always-on-top. When scrcpy is in
/// background, we remove always-on-top so float doesn't cover other apps.
fn sync_float_z_order(app: &tauri::AppHandle, scrcpy_is_frontmost: bool) {
    if let Some(float) = app.get_webview_window("float") {
        // When scrcpy is frontmost, keep float always-on-top.
        // When scrcpy is background, remove always-on-top so float goes back too.
        let _ = float.set_always_on_top(scrcpy_is_frontmost);
    }
}

fn spawn_lifecycle_watcher(
    app: tauri::AppHandle,
    child_slot: Arc<Mutex<Option<tokio::process::Child>>>,
    follower_slot: Arc<Mutex<Option<tokio::task::AbortHandle>>>,
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
                // Abort the follower so it doesn't poll a dead PID until its
                // own miss budget runs out.
                if let Some(h) = follower_slot.lock().await.take() {
                    h.abort();
                }
                // Backend owns the float window — hide it directly.
                hide_float_window(&app);
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
        abort_follower(&state).await;
        hide_float_window(&app);
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

/// USB-bootstrapped wireless (method A, step 1): read the device's Wi-Fi IP,
/// then switch it into TCP/IP listening mode on port 5555. We read the IP
/// FIRST because `adb tcpip` restarts adbd — the device drops off USB for a
/// moment, so a read issued right after would race the restart and come back
/// empty (the bug that left the UI field blank). `serial` is whitelist-
/// validated before it reaches argv (injection guard).
#[tauri::command]
async fn enable_tcpip(serial: String) -> AppResult<String> {
    if !scrcpy::is_valid_serial(&serial) {
        return Err(AppError::WirelessConnectFailed(format!("非法序列号: {serial}")));
    }
    let adb = adb::detect_adb_path().ok_or(AppError::AdbNotFound)?;

    // Read the device's Wi-Fi IPv4 BEFORE restarting adbd (USB still steady).
    // Best-effort: if we can't read it, the user types the IP manually.
    let ip = read_device_ip(&adb, &serial).await.unwrap_or_default();

    // Now restart adbd in TCP mode on 5555.
    let out = tokio::process::Command::new(&adb)
        .args(["-s", &serial, "tcpip", "5555"])
        .output()
        .await?;
    if !out.status.success() {
        return Err(AppError::WirelessConnectFailed(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }

    Ok(ip)
}

/// Read a device's Wi-Fi IPv4 over adb. Tries `wlan0` first (the usual Android
/// Wi-Fi interface), then falls back to whatever interface owns the default
/// route — covers devices where Wi-Fi isn't named wlan0.
async fn read_device_ip(adb: &std::path::Path, serial: &str) -> Option<String> {
    // Primary: explicit wlan0.
    let out = tokio::process::Command::new(adb)
        .args(["-s", serial, "shell", "ip", "-f", "inet", "addr", "show", "wlan0"])
        .output()
        .await
        .ok()?;
    if let Some(ip) = parse_wlan_ip(&String::from_utf8_lossy(&out.stdout)) {
        return Some(ip);
    }

    // Fallback: the source IP of the default route ("ip route get 1" prints
    // "... src 192.168.x.y ..."). Catches non-wlan0 Wi-Fi names.
    let out = tokio::process::Command::new(adb)
        .args(["-s", serial, "shell", "ip", "route", "get", "1"])
        .output()
        .await
        .ok()?;
    parse_route_src_ip(&String::from_utf8_lossy(&out.stdout))
}

/// Wireless pairing (method B, Android 11+): `adb pair <ip>:<port> <code>`.
/// Pairing uses a DIFFERENT port than the later connection, so this does NOT
/// connect — the caller follows up with connect_wireless on port 5555 (or the
/// device's shown connect port). All inputs are validated before argv.
#[tauri::command]
async fn pair_wireless(ip: String, port: String, code: String) -> AppResult<()> {
    if !scrcpy::is_valid_ip(&ip) {
        return Err(AppError::WirelessConnectFailed(format!("非法地址: {ip}")));
    }
    if !is_valid_port(&port) {
        return Err(AppError::WirelessConnectFailed(format!("非法端口: {port}")));
    }
    if !is_valid_pairing_code(&code) {
        return Err(AppError::WirelessConnectFailed("配对码应为 6 位数字".into()));
    }
    let adb = adb::detect_adb_path().ok_or(AppError::AdbNotFound)?;
    // `ip` here is a bare host (validated); pairing always carries an explicit
    // port, so build host:port regardless of whether ip already had one.
    let host = ip.split(':').next().unwrap_or(&ip);
    let target = format!("{host}:{port}");
    let out = tokio::process::Command::new(adb)
        .args(["pair", &target, &code])
        .output()
        .await?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.contains("Successfully paired") {
        Ok(())
    } else {
        // adb prints failures to stdout ("Failed: ...") or stderr.
        let detail = if stdout.trim().is_empty() {
            String::from_utf8_lossy(&out.stderr).trim().to_string()
        } else {
            stdout.trim().to_string()
        };
        Err(AppError::WirelessConnectFailed(detail))
    }
}

/// Extract an IPv4 from `ip addr show wlan0` output: the token after "inet",
/// stripped of its /prefix. Pure — unit-tested.
fn parse_wlan_ip(output: &str) -> Option<String> {
    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("inet ") {
            // rest = "192.168.1.23/24 brd ..."
            let addr = rest.split_whitespace().next()?;
            let ip = addr.split('/').next()?;
            if !ip.is_empty() {
                return Some(ip.to_string());
            }
        }
    }
    None
}

/// Extract the source IPv4 from `ip route get 1` output. The line looks like
/// "1.0.0.0 via 192.168.1.1 dev wlan0 src 192.168.1.23 uid 0" — we want the
/// token after "src". Pure — unit-tested.
fn parse_route_src_ip(output: &str) -> Option<String> {
    let mut toks = output.split_whitespace();
    while let Some(t) = toks.next() {
        if t == "src" {
            let ip = toks.next()?;
            if !ip.is_empty() {
                return Some(ip.to_string());
            }
        }
    }
    None
}

/// A TCP port string: 1–65535, digits only.
fn is_valid_port(port: &str) -> bool {
    matches!(port.parse::<u32>(), Ok(n) if (1..=65535).contains(&n))
}

/// adb pairing codes are exactly 6 digits.
fn is_valid_pairing_code(code: &str) -> bool {
    code.len() == 6 && code.chars().all(|c| c.is_ascii_digit())
}

#[tauri::command]
async fn send_key(
    action: keyinject::KeyAction,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> AppResult<()> {
    use keyinject::KeyAction;
    // The device serial is needed for every adb-routed action.
    let serial = state
        .serial
        .lock()
        .await
        .clone()
        .ok_or(AppError::DeviceNotConnected)?;

    match action {
        // Close kills scrcpy — no device round-trip.
        KeyAction::Close => stop_scrcpy(app, state).await,
        // Screenshot pulls a PNG to the desktop.
        KeyAction::Screenshot => take_screenshot_inner(&serial).await.map(|_| ()),
        // Rotate is a multi-step settings read-modify-write.
        KeyAction::Rotate => {
            let adb_path = adb::detect_adb_path().ok_or(AppError::AdbNotFound)?;
            keyinject::rotate_screen(&serial, &adb_path).await
        }
        // Everything else is a straight adb keyevent / shell command.
        _ => {
            let adb_path = adb::detect_adb_path().ok_or(AppError::AdbNotFound)?;
            keyinject::inject(action, &serial, &adb_path).await
        }
    }
}

async fn take_screenshot_inner(serial: &str) -> AppResult<String> {
    let adb_path = adb::detect_adb_path().ok_or(AppError::AdbNotFound)?;
    let local_path = desktop_path(serial, "png")?;

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

/// `~/Desktop/scrcpy-<serial>-<unix_ts>.<ext>` (PRD §3.3 naming).
fn desktop_path(serial: &str, ext: &str) -> AppResult<String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let home = std::env::var("HOME").map_err(|e| AppError::Io(e.to_string()))?;
    Ok(format!("{home}/Desktop/scrcpy-{serial}-{ts}.{ext}"))
}

/// Toggle screen recording. Recording is a Runtime Action (ARCHITECTURE §2):
/// scrcpy can only record if `--record` is passed at launch, and the .mp4 is
/// finalized only when that process exits. So:
///   - start: relaunch with base_args + `--record=<path>`
///   - stop:  relaunch with base_args alone (the recording process exits,
///            finalizing the file; mirroring continues uninterrupted)
/// Returns the new state so the float panel updates from the return value —
/// no separate event needed, since only this click changes recording state.
#[tauri::command]
async fn toggle_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> AppResult<RecordingState> {
    let serial = state
        .serial
        .lock()
        .await
        .clone()
        .ok_or(AppError::DeviceNotConnected)?;
    let base = state.base_args.lock().await.clone();
    let screen_off = *state.screen_off.lock().await;
    let host_audio = *state.host_audio.lock().await;
    let always_on_top = *state.always_on_top.lock().await;
    let currently_recording = state.recording.lock().await.is_some();

    if currently_recording {
        // STOP: relaunch without --record. The old (recording) child is killed
        // inside spawn_and_swap, which finalizes the .mp4. Other runtime
        // toggles are preserved across the relaunch by feeding them all back
        // into the composer.
        let saved = state.recording.lock().await.clone();
        let args = compose_runtime_args(&base, screen_off, host_audio, always_on_top, None);
        spawn_and_swap(&serial, &args, &app, &state).await?;
        *state.recording.lock().await = None;
        Ok(RecordingState { recording: false, saved_path: saved })
    } else {
        // START: relaunch with --record appended, written into the user's
        // chosen directory (or the default if none has been picked).
        let dir = state.record_dir.lock().await.clone();
        let path = recording_path(dir.as_deref(), &serial)?;
        let args = compose_runtime_args(&base, screen_off, host_audio, always_on_top, Some(&path));
        spawn_and_swap(&serial, &args, &app, &state).await?;
        *state.recording.lock().await = Some(path);
        Ok(RecordingState { recording: true, saved_path: None })
    }
}

/// Toggle phone-screen-off mirroring. Same Runtime-Action shape as recording:
/// the flag is launch-time only, so we relaunch scrcpy with base_args + every
/// currently active runtime flag. `--turn-screen-off` darkens the phone's
/// physical display while mirroring keeps streaming; it must be paired with
/// `--stay-awake` or Android will lock the device once the user backs out.
#[tauri::command]
async fn toggle_screen_off(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> AppResult<ScreenOffState> {
    let serial = state
        .serial
        .lock()
        .await
        .clone()
        .ok_or(AppError::DeviceNotConnected)?;
    let base = state.base_args.lock().await.clone();
    let currently_off = *state.screen_off.lock().await;
    let host_audio = *state.host_audio.lock().await;
    let always_on_top = *state.always_on_top.lock().await;
    let record_path = state.recording.lock().await.clone();

    let next_off = !currently_off;

    // When enabling screen_off, also set system screen timeout to maximum to
    // prevent device sleep. When disabling, restore the original value.
    if next_off {
        // Capture original timeout if not already stored.
        if state.original_screen_timeout.lock().await.is_none() {
            let original = read_screen_timeout(&serial).await.ok();
            *state.original_screen_timeout.lock().await = original;
        }
        // Set to max (2147483647 ms = ~24 days) to prevent auto-sleep.
        let _ = set_screen_timeout(&serial, "2147483647").await;
    } else {
        // Restore original timeout.
        if let Some(original) = state.original_screen_timeout.lock().await.take() {
            let _ = set_screen_timeout(&serial, &original).await;
        }
    }

    let args = compose_runtime_args(&base, next_off, host_audio, always_on_top, record_path.as_deref());
    spawn_and_swap(&serial, &args, &app, &state).await?;
    *state.screen_off.lock().await = next_off;
    Ok(ScreenOffState { screen_off: next_off })
}

/// Toggle whether the Mac plays the device's audio. Default true (scrcpy's
/// own default — audio streams over the host). Switching to false relaunches
/// with `--no-audio` so the device keeps playing through its own speakers.
#[tauri::command]
async fn toggle_audio_host(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> AppResult<AudioHostState> {
    let serial = state
        .serial
        .lock()
        .await
        .clone()
        .ok_or(AppError::DeviceNotConnected)?;
    let base = state.base_args.lock().await.clone();
    let screen_off = *state.screen_off.lock().await;
    let currently = *state.host_audio.lock().await;
    let always_on_top = *state.always_on_top.lock().await;
    let record_path = state.recording.lock().await.clone();

    let next = !currently;
    let args = compose_runtime_args(&base, screen_off, next, always_on_top, record_path.as_deref());
    spawn_and_swap(&serial, &args, &app, &state).await?;
    *state.host_audio.lock().await = next;
    Ok(AudioHostState { host_audio: next })
}

/// Toggle always-on-top for scrcpy window. When enabled, scrcpy window stays
/// above all other windows even when switching apps. Useful for side-by-side
/// workflows. Relaunches scrcpy with --always-on-top flag.
#[tauri::command]
async fn toggle_always_on_top(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> AppResult<AlwaysOnTopState> {
    let serial = state
        .serial
        .lock()
        .await
        .clone()
        .ok_or(AppError::DeviceNotConnected)?;
    let base = state.base_args.lock().await.clone();
    let screen_off = *state.screen_off.lock().await;
    let host_audio = *state.host_audio.lock().await;
    let currently = *state.always_on_top.lock().await;
    let record_path = state.recording.lock().await.clone();

    let next = !currently;
    let args = compose_runtime_args(&base, screen_off, host_audio, next, record_path.as_deref());
    spawn_and_swap(&serial, &args, &app, &state).await?;
    *state.always_on_top.lock().await = next;
    Ok(AlwaysOnTopState { always_on_top: next })
}

/// Set the directory where future screen recordings land. The path is
/// pre-validated for writability — if the check fails, we fall back to the
/// previous effective directory (or the default) and report `accepted=false`
/// so the UI can show the reason without surprising the user later when a
/// recording attempt would have failed.
///
/// Passing `path=None` (or empty) resets to the default (~/Desktop).
#[tauri::command]
async fn set_record_dir(
    path: Option<String>,
    state: tauri::State<'_, AppState>,
) -> AppResult<RecordDirState> {
    let trimmed = path
        .as_deref()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());

    let default = default_record_dir().unwrap_or_else(|| "/tmp".to_string());

    match trimmed {
        // Explicit reset → default.
        None => {
            *state.record_dir.lock().await = None;
            Ok(RecordDirState {
                effective: default,
                accepted: true,
                message: None,
            })
        }
        Some(p) => match dir_writable_reason(&p) {
            None => {
                *state.record_dir.lock().await = Some(p.clone());
                Ok(RecordDirState {
                    effective: p,
                    accepted: true,
                    message: None,
                })
            }
            Some(reason) => {
                // Reject: keep the previous setting, hand back the effective
                // dir (so the UI can show what's actually in force).
                let prev = state.record_dir.lock().await.clone();
                let effective = prev.unwrap_or(default);
                Ok(RecordDirState {
                    effective,
                    accepted: false,
                    message: Some(reason),
                })
            }
        },
    }
}

/// Open the physical keyboard settings on the device. This is the same
/// action scrcpy performs when the user presses MOD+k in the mirror window.
/// Useful when keyboard input isn't working — the user needs to configure
/// the keyboard layout once for UHID mode to work.
#[tauri::command]
async fn open_keyboard_settings(state: tauri::State<'_, AppState>) -> AppResult<()> {
    let serial = state
        .serial
        .lock()
        .await
        .as_ref()
        .ok_or(AppError::DeviceNotConnected)?
        .clone();

    let output = tokio::process::Command::new(
        adb::detect_adb_path().ok_or(AppError::AdbNotFound)?,
    )
    .args([
        "-s",
        &serial,
        "shell",
        "am",
        "start",
        "-a",
        "android.settings.HARD_KEYBOARD_SETTINGS",
    ])
    .output()
    .await
    .map_err(|e| AppError::Io(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::KeyInjectFailed(format!(
            "打开物理键盘设置失败: {stderr}"
        )));
    }

    Ok(())
}

/// Read the device's current screen_off_timeout setting (in milliseconds).
/// Returns the value as a string, or an error if it can't be read.
async fn read_screen_timeout(serial: &str) -> AppResult<String> {
    let adb_path = adb::detect_adb_path().ok_or(AppError::AdbNotFound)?;
    let output = tokio::process::Command::new(adb_path)
        .args([
            "-s",
            serial,
            "shell",
            "settings",
            "get",
            "system",
            "screen_off_timeout",
        ])
        .output()
        .await
        .map_err(|e| AppError::Io(e.to_string()))?;

    if !output.status.success() {
        return Err(AppError::Io("无法读取屏幕超时设置".into()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Set the device's screen_off_timeout to the given value (in milliseconds).
async fn set_screen_timeout(serial: &str, timeout_ms: &str) -> AppResult<()> {
    let adb_path = adb::detect_adb_path().ok_or(AppError::AdbNotFound)?;
    let output = tokio::process::Command::new(adb_path)
        .args([
            "-s",
            serial,
            "shell",
            "settings",
            "put",
            "system",
            "screen_off_timeout",
            timeout_ms,
        ])
        .output()
        .await
        .map_err(|e| AppError::Io(e.to_string()))?;

    if !output.status.success() {
        return Err(AppError::Io("无法设置屏幕超时".into()));
    }

    Ok(())
}

/// Default directory for screen recordings — `~/Desktop`, matching the
/// previous hard-coded behaviour of desktop_path().
fn default_record_dir() -> Option<String> {
    std::env::var("HOME").ok().map(|h| format!("{h}/Desktop"))
}

/// Check whether `path` is a directory we can actually write into. Returns
/// None if writable, or a short user-facing reason otherwise.
fn dir_writable_reason(path: &str) -> Option<String> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return Some(format!("无法访问: {e}")),
    };
    if !meta.is_dir() {
        return Some("不是目录".to_string());
    }
    // Probe by creating and removing a hidden temp file. metadata().permissions()
    // alone doesn't reflect macOS sandbox/TCC restrictions — only an actual
    // write does.
    let probe = std::path::Path::new(path).join(".scrcpy-mac-ui-write-test");
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            None
        }
        Err(e) => Some(format!("目录不可写: {e}")),
    }
}

/// Build the full scrcpy argv for a relaunch driven by runtime toggles.
/// Pure — unit-tested so the flag construction is verified without spawning
/// scrcpy. Order: base preset → audio off → screen-off pair → record (record
/// last so a failed spawn surfaces a `--record` parse error against a known
/// prefix).
fn compose_runtime_args(
    base: &[String],
    screen_off: bool,
    host_audio: bool,
    always_on_top: bool,
    record_path: Option<&str>,
) -> Vec<String> {
    let mut args = base.to_vec();
    if !host_audio {
        // Default scrcpy behaviour is host-audio; --no-audio means "let the
        // device speakers play it" (we don't capture the audio stream at all).
        args.push("--no-audio".into());
    }
    if screen_off {
        // --turn-screen-off alone lets the OS lock the device after a moment;
        // --stay-awake keeps it unlocked while charging/connected so mirroring
        // remains responsive. Pairing them is the documented scrcpy idiom.
        args.push("--turn-screen-off".into());
        args.push("--stay-awake".into());
    }
    if always_on_top {
        // --always-on-top keeps scrcpy window above all other windows, even
        // when switching to other apps. Useful for side-by-side workflows.
        args.push("--always-on-top".into());
    }
    if let Some(path) = record_path {
        args.push(format!("--record={path}"));
    }
    args
}

/// Build a full path for a new recording: `<dir>/scrcpy-<serial>-<ts>.mp4`,
/// where `dir` is the user-chosen directory or `~/Desktop` if unset.
fn recording_path(dir: Option<&str>, serial: &str) -> AppResult<String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dir = match dir {
        Some(d) => d.to_string(),
        None => default_record_dir().ok_or_else(|| AppError::Io("HOME not set".into()))?,
    };
    Ok(format!("{dir}/scrcpy-{serial}-{ts}.mp4"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
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
            enable_tcpip,
            pair_wireless,
            send_key,
            toggle_recording,
            toggle_screen_off,
            toggle_audio_host,
            toggle_always_on_top,
            set_record_dir,
            open_keyboard_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_path_uses_mp4_extension_and_serial() {
        let p = desktop_path("R5CX21RJ6MX", "mp4").unwrap();
        assert!(p.ends_with(".mp4"));
        assert!(p.contains("scrcpy-R5CX21RJ6MX-"));
        assert!(p.contains("/Desktop/"));
    }

    #[test]
    fn recording_path_uses_custom_dir_when_provided() {
        let p = recording_path(Some("/tmp/foo"), "R5CX21RJ6MX").unwrap();
        assert!(p.starts_with("/tmp/foo/"));
        assert!(p.ends_with(".mp4"));
        assert!(p.contains("scrcpy-R5CX21RJ6MX-"));
    }

    #[test]
    fn recording_path_falls_back_to_desktop_when_dir_is_none() {
        let p = recording_path(None, "ABCD1234").unwrap();
        // Same shape as the previous desktop_path behaviour.
        assert!(p.contains("/Desktop/"));
        assert!(p.ends_with(".mp4"));
        assert!(p.contains("scrcpy-ABCD1234-"));
    }

    #[test]
    fn dir_writable_reason_accepts_existing_writable_dir() {
        // The std::env::temp_dir() is writable for the test runner.
        let tmp = std::env::temp_dir();
        let reason = dir_writable_reason(tmp.to_string_lossy().as_ref());
        assert!(reason.is_none(), "expected writable, got {reason:?}");
    }

    #[test]
    fn dir_writable_reason_rejects_missing_path() {
        let reason = dir_writable_reason("/this/path/should/not/exist/xyz");
        assert!(reason.is_some());
    }

    #[test]
    fn dir_writable_reason_rejects_a_regular_file() {
        // Any existing file works — Cargo.toml is always present in the crate.
        let reason = dir_writable_reason("Cargo.toml");
        assert_eq!(reason.as_deref(), Some("不是目录"));
    }

    #[test]
    fn record_dir_state_serializes_camelcase() {
        let s = RecordDirState {
            effective: "/x".into(),
            accepted: false,
            message: Some("目录不可写".into()),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"effective\":\"/x\""));
        assert!(json.contains("\"accepted\":false"));
        assert!(json.contains("\"message\":\"目录不可写\""));
    }

    #[test]
    fn scrcpy_window_args_anchor_at_monitor_origin() {
        // Primary monitor (origin 0,0): window pinned at the layout insets.
        let a = layout::scrcpy_window_args(0.0, 0.0);
        assert!(a.contains(&"--window-x=60".to_string()));
        assert!(a.contains(&"--window-y=40".to_string()));
        assert!(a.contains(&"--window-height=900".to_string()));
    }

    #[test]
    fn scrcpy_window_args_offset_by_negative_origin() {
        // A monitor to the LEFT of primary has a negative origin; the window x
        // must shift with it so the mirror lands on that screen.
        let a = layout::scrcpy_window_args(-2056.0, 0.0);
        assert!(a.contains(&"--window-x=-1996".to_string())); // -2056 + 60
    }

    #[test]
    fn float_snaps_to_right_of_mirror() {
        // Float x = origin + mirror inset + reserved width + gap.
        let (x, y) = layout::float_position(0.0, 0.0);
        assert_eq!(x, 60.0 + 430.0 + 8.0); // 498
        assert_eq!(y, 40.0); // aligned with the mirror top
    }

    #[test]
    fn float_position_tracks_monitor_origin() {
        let (x0, _) = layout::float_position(0.0, 0.0);
        let (x1, y1) = layout::float_position(1000.0, 50.0);
        assert_eq!(x1 - x0, 1000.0); // shifts fully with the monitor
        assert_eq!(y1, 50.0 + 40.0);
    }

    #[test]
    fn desktop_path_screenshot_uses_png() {
        let p = desktop_path("ABCD1234", "png").unwrap();
        assert!(p.ends_with(".png"));
        assert!(p.contains("scrcpy-ABCD1234-"));
    }

    #[test]
    fn compose_runtime_args_passthrough_when_no_toggles_active() {
        let base = vec!["--max-size=1920".to_string(), "--max-fps=60".to_string()];
        // host_audio=true is the default ("Mac plays the audio"); no flag added.
        let args = compose_runtime_args(&base, false, true, false, None);
        // Plain mirror relaunch — base args verbatim, no extra flags.
        assert_eq!(args, base);
    }

    #[test]
    fn compose_runtime_args_appends_record_flag_after_base() {
        let base = vec!["--max-size=1920".to_string(), "--max-fps=60".to_string()];
        let args =
            compose_runtime_args(&base, false, true, false, Some("/Users/x/Desktop/scrcpy-DEV-1.mp4"));
        // Base args preserved in order, --record appended last.
        assert_eq!(args[0], "--max-size=1920");
        assert_eq!(args[1], "--max-fps=60");
        assert_eq!(args[2], "--record=/Users/x/Desktop/scrcpy-DEV-1.mp4");
    }

    #[test]
    fn compose_runtime_args_works_with_empty_base() {
        let args = compose_runtime_args(&[], false, true, false, Some("/tmp/a.mp4"));
        assert_eq!(args, vec!["--record=/tmp/a.mp4"]);
    }

    #[test]
    fn compose_runtime_args_pairs_screen_off_with_stay_awake() {
        // The screen-off idiom in scrcpy is `--turn-screen-off --stay-awake`
        // together — alone, Android locks the device after a moment and the
        // mirror stops being responsive. Verifying the pair so a future edit
        // can't drop one and silently break the feature.
        let args = compose_runtime_args(&[], true, true, false, None);
        assert!(args.iter().any(|a| a == "--turn-screen-off"));
        assert!(args.iter().any(|a| a == "--stay-awake"));
    }

    #[test]
    fn compose_runtime_args_combines_screen_off_with_record() {
        let base = vec!["--max-fps=60".to_string()];
        let args = compose_runtime_args(&base, true, true, false, Some("/tmp/a.mp4"));
        // Base first, screen-off pair next, record last. (host_audio=true → no
        // --no-audio inserted between them.)
        assert_eq!(args[0], "--max-fps=60");
        assert_eq!(args[1], "--turn-screen-off");
        assert_eq!(args[2], "--stay-awake");
        assert_eq!(args[3], "--record=/tmp/a.mp4");
    }

    #[test]
    fn compose_runtime_args_adds_no_audio_when_not_hosting() {
        // host_audio=false ⇒ device speakers play the audio (scrcpy doesn't
        // capture the stream at all).
        let args = compose_runtime_args(&[], false, false, false, None);
        assert_eq!(args, vec!["--no-audio"]);
    }

    #[test]
    fn compose_runtime_args_no_audio_position_between_base_and_screen_off() {
        // --no-audio sits AFTER the user's preset but BEFORE --turn-screen-off
        // and --record, so a malformed preset never gets shoved past our flags.
        let base = vec!["--max-fps=60".to_string()];
        let args = compose_runtime_args(&base, true, false, false, Some("/tmp/a.mp4"));
        assert_eq!(args[0], "--max-fps=60");
        assert_eq!(args[1], "--no-audio");
        assert_eq!(args[2], "--turn-screen-off");
        assert_eq!(args[3], "--stay-awake");
        assert_eq!(args[4], "--record=/tmp/a.mp4");
    }

    #[test]
    fn compose_runtime_args_adds_always_on_top_when_enabled() {
        // always_on_top=true ⇒ --always-on-top flag keeps window on top.
        let args = compose_runtime_args(&[], false, true, true, None);
        assert!(args.iter().any(|a| a == "--always-on-top"));
    }

    #[test]
    fn audio_host_state_serializes_camelcase() {
        let s = AudioHostState { host_audio: false };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"hostAudio\":false"));
        assert!(!json.contains("host_audio"));
    }

    #[test]
    fn parse_wlan_ip_extracts_address_without_prefix() {
        let out = "34: wlan0: <BROADCAST> mtu 1500\n    inet 192.168.1.23/24 brd 192.168.1.255 scope global wlan0\n       valid_lft forever";
        assert_eq!(parse_wlan_ip(out), Some("192.168.1.23".to_string()));
    }

    #[test]
    fn parse_wlan_ip_returns_none_when_no_inet_line() {
        let out = "34: wlan0: <BROADCAST> mtu 1500\n    link/ether aa:bb:cc:dd:ee:ff";
        assert_eq!(parse_wlan_ip(out), None);
    }

    #[test]
    fn parse_route_src_ip_extracts_src_token() {
        let out = "1.0.0.0 via 192.168.1.1 dev wlan0 src 192.168.1.23 uid 0\n    cache";
        assert_eq!(parse_route_src_ip(out), Some("192.168.1.23".to_string()));
    }

    #[test]
    fn parse_route_src_ip_returns_none_without_src() {
        let out = "unreachable 1.0.0.0 dev lo";
        assert_eq!(parse_route_src_ip(out), None);
    }

    #[test]
    fn is_valid_port_bounds() {
        assert!(is_valid_port("1"));
        assert!(is_valid_port("5555"));
        assert!(is_valid_port("65535"));
        assert!(!is_valid_port("0"));
        assert!(!is_valid_port("65536"));
        assert!(!is_valid_port("abc"));
        assert!(!is_valid_port(""));
        assert!(!is_valid_port("-1"));
    }

    #[test]
    fn is_valid_pairing_code_is_six_digits() {
        assert!(is_valid_pairing_code("123456"));
        assert!(!is_valid_pairing_code("12345")); // too short
        assert!(!is_valid_pairing_code("1234567")); // too long
        assert!(!is_valid_pairing_code("12345a")); // non-digit
        assert!(!is_valid_pairing_code("")); // empty
    }

    #[test]
    fn screen_off_state_serializes_camelcase() {
        // Frontend reads `screenOff`, not `screen_off` — verify the rename.
        let s = ScreenOffState { screen_off: true };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"screenOff\":true"));
        assert!(!json.contains("screen_off"));
    }

    #[test]
    fn recording_state_serializes_camelcase() {
        // Frontend reads `savedPath`, not `saved_path` — verify the rename.
        let s = RecordingState {
            recording: false,
            saved_path: Some("/x/y.mp4".into()),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"savedPath\":\"/x/y.mp4\""));
        assert!(json.contains("\"recording\":false"));
        assert!(!json.contains("saved_path"));
    }

    #[test]
    fn recording_state_null_saved_path_when_starting() {
        let s = RecordingState { recording: true, saved_path: None };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"savedPath\":null"));
        assert!(json.contains("\"recording\":true"));
    }
}
