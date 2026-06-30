// scrcpy process management (PRD §5.2, §5.3, C1).
//
// Lifecycle interlock:
//   launch() ──spawn──▶ child ──┬──▶ tokio task drains stdout ─▶ ring buffer
//                               └──▶ tokio task drains stderr ─▶ ring buffer
//   (draining is mandatory: an unread 64KB pipe buffer would block scrcpy — C1)
//
//   kill(): SIGTERM ─▶ wait 2s ─▶ still alive? ─▶ SIGKILL  (§5.3)
//
// Security: every argument is passed as argv, never shell-interpolated (§5.2).
// Serial/IP inputs are validated against a whitelist before use.

use crate::error::{AppError, AppResult};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};

const RING_CAPACITY: usize = 200;

// Validate an adb serial. Two shapes are allowed:
//   - USB serial: alphanumeric, 8–32 chars (PRD §5.2).
//   - Wireless serial: an IPv4[:port], e.g. "192.168.1.9:5555" — this is what
//     `adb devices` reports for a network-connected device, so launching /
//     keying a wireless device must accept it.
// Both reach adb via argv (never a shell), so the dots/colons are safe.
pub fn is_valid_serial(serial: &str) -> bool {
    let len = serial.len();
    let alphanumeric = (8..=32).contains(&len) && serial.chars().all(|c| c.is_ascii_alphanumeric());
    alphanumeric || is_valid_ip(serial)
}

// Validate an IPv4[:port] target (PRD §5.2).
pub fn is_valid_ip(input: &str) -> bool {
    let (host, port) = match input.split_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (input, None),
    };
    let octets: Vec<&str> = host.split('.').collect();
    if octets.len() != 4 {
        return false;
    }
    if !octets.iter().all(|o| o.parse::<u8>().is_ok()) {
        return false;
    }
    match port {
        None => true,
        Some(p) => matches!(p.parse::<u32>(), Ok(n) if (1..=65535).contains(&n)),
    }
}

fn scrcpy_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/opt/homebrew/bin/scrcpy"),
        PathBuf::from("/usr/local/bin/scrcpy"),
    ]
}

pub fn detect_scrcpy_path() -> Option<PathBuf> {
    scrcpy_candidates().into_iter().find(|p| p.exists())
}

// Build the full scrcpy argv for a device + preset args.
// Returns an error if the serial fails validation (injection guard).
pub fn build_args(serial: &str, preset_args: &[String]) -> AppResult<Vec<String>> {
    if !is_valid_serial(serial) {
        return Err(AppError::ScrcpyLaunchFailed(format!(
            "非法设备序列号: {serial}"
        )));
    }
    let mut args = vec!["--serial".to_string(), serial.to_string()];
    args.extend_from_slice(preset_args);
    Ok(args)
}

// A bounded, shareable log buffer for child stdout/stderr.
#[derive(Clone, Default)]
pub struct LogRing {
    inner: Arc<Mutex<std::collections::VecDeque<String>>>,
}

impl LogRing {
    pub fn push(&self, line: String) {
        let mut q = self.inner.lock().unwrap();
        if q.len() == RING_CAPACITY {
            q.pop_front();
        }
        q.push_back(line);
    }

    pub fn last(&self) -> Option<String> {
        self.inner.lock().unwrap().back().cloned()
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.inner.lock().unwrap().iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// Spawn scrcpy and start draining its stdout/stderr (C1: prevents pipe-buffer
// deadlock). Returns the child handle and the stderr ring (for error surfacing).
pub async fn launch(serial: &str, preset_args: &[String]) -> AppResult<(tokio::process::Child, LogRing)> {
    let bin = detect_scrcpy_path()
        .ok_or_else(|| AppError::ScrcpyLaunchFailed("未找到 scrcpy".to_string()))?;
    let args = build_args(serial, preset_args)?;

    let mut cmd = tokio::process::Command::new(bin);
    cmd.args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // A Finder-launched .app does NOT inherit the shell PATH, so a bundled
    // build can't find `adb` on its own — scrcpy then fails with "server
    // connection failed". We resolve adb's absolute path ourselves and hand it
    // to scrcpy two ways: the ADB env var (scrcpy reads it to locate adb) and
    // a PATH prefix (covers any other tool scrcpy shells out to).
    if let Some(adb) = crate::adb::detect_adb_path() {
        cmd.env("ADB", &adb);
        if let Some(dir) = adb.parent() {
            let existing = std::env::var_os("PATH").unwrap_or_default();
            let mut paths: Vec<std::path::PathBuf> = vec![dir.to_path_buf()];
            paths.extend(std::env::split_paths(&existing));
            if let Ok(joined) = std::env::join_paths(paths) {
                cmd.env("PATH", joined);
            }
        }
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::ScrcpyLaunchFailed(e.to_string()))?;

    let stderr_ring = LogRing::default();

    if let Some(stdout) = child.stdout.take() {
        let ring = LogRing::default();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                ring.push(line);
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let ring = stderr_ring.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                ring.push(line);
            }
        });
    }

    Ok((child, stderr_ring))
}

// Gracefully stop a child: send SIGTERM, wait up to 2s for the process to
// finalize (critical for `--record` to flush the moov atom — without it the
// resulting .mp4 is unplayable), then force-kill if still alive.
//
// Why not tokio::Child::start_kill: despite older docs claiming SIGTERM, it
// actually sends SIGKILL on Unix (verified in tokio 1.x source). SIGKILL gives
// the process no chance to flush — recordings finalize, but only if SIGTERM
// reaches them first. We use libc::kill directly to get true SIGTERM.
pub async fn kill(mut child: tokio::process::Child) -> AppResult<()> {
    // Already exited?
    if let Ok(Some(_)) = child.try_wait() {
        return Ok(());
    }
    if let Some(pid) = child.id() {
        // SAFETY: libc::kill is safe to call with any pid + signal. The pid
        // came from a process WE spawned; even if it has been reaped, kill()
        // just returns ESRCH which we ignore — try_wait below catches exit.
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
    }
    // Poll for up to 2 seconds — scrcpy needs time to flush moov on --record.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    // Still alive → force kill and reap.
    let _ = child.kill().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_serials_accepted() {
        assert!(is_valid_serial("R5CX21RJ6MX"));
        assert!(is_valid_serial("12345678"));
    }

    #[test]
    fn wireless_serials_accepted() {
        // `adb devices` reports network devices as IP:port — must pass so a
        // wireless device can launch scrcpy / receive key events.
        assert!(is_valid_serial("192.168.137.189:5555"));
        assert!(is_valid_serial("10.0.0.7:5555"));
        // Bare IP (no port) also valid.
        assert!(is_valid_serial("192.168.1.9"));
    }

    #[test]
    fn invalid_serials_rejected() {
        assert!(!is_valid_serial("short")); // < 8
        assert!(!is_valid_serial("abc; rm -rf /")); // metacharacters
        assert!(!is_valid_serial("$(whoami)"));
        assert!(!is_valid_serial(&"a".repeat(33))); // > 32
        assert!(!is_valid_serial("999.1.1.1:5555")); // octet > 255 — not a real IP
        assert!(!is_valid_serial("1.2.3.4; rm")); // injection via fake IP
    }

    #[test]
    fn valid_ips_accepted() {
        assert!(is_valid_ip("192.168.1.100"));
        assert!(is_valid_ip("192.168.1.100:5555"));
        assert!(is_valid_ip("10.0.0.1"));
    }

    #[test]
    fn invalid_ips_rejected() {
        assert!(!is_valid_ip("999.1.1.1")); // octet > 255
        assert!(!is_valid_ip("192.168.1")); // too few octets
        assert!(!is_valid_ip("192.168.1.1:70000")); // port out of range
        assert!(!is_valid_ip("not-an-ip"));
        assert!(!is_valid_ip("192.168.1.1; rm -rf /"));
    }

    #[test]
    fn build_args_prepends_serial_flag() {
        let preset = vec!["--max-size=1920".to_string()];
        let args = build_args("R5CX21RJ6MX", &preset).unwrap();
        assert_eq!(args, vec!["--serial", "R5CX21RJ6MX", "--max-size=1920"]);
    }

    #[test]
    fn build_args_rejects_bad_serial() {
        let preset = vec![];
        assert!(build_args("bad; rm", &preset).is_err());
    }

    #[test]
    fn ring_caps_at_capacity() {
        let ring = LogRing::default();
        for i in 0..(RING_CAPACITY + 50) {
            ring.push(format!("line {i}"));
        }
        assert_eq!(ring.len(), RING_CAPACITY);
        assert_eq!(ring.last().unwrap(), format!("line {}", RING_CAPACITY + 49));
    }

    #[test]
    fn ring_starts_empty() {
        let ring = LogRing::default();
        assert!(ring.is_empty());
        assert!(ring.last().is_none());
    }

    #[tokio::test]
    async fn kill_returns_for_already_exited_process() {
        // `true` exits immediately; kill() must handle the reaped case.
        let child = tokio::process::Command::new("true").spawn().unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(kill(child).await.is_ok());
    }

    #[tokio::test]
    async fn kill_terminates_long_running_process() {
        // `sleep 60` is alive; kill() must terminate it via the ladder.
        let child = tokio::process::Command::new("sleep").arg("60").spawn().unwrap();
        let start = tokio::time::Instant::now();
        assert!(kill(child).await.is_ok());
        // Must return well under the 2s SIGTERM grace (sleep dies on first signal).
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }

    #[tokio::test]
    async fn kill_actually_sends_sigterm_not_sigkill() {
        // Regression guard for the recording-finalization bug: scrcpy needs
        // SIGTERM (catchable) to flush --record's moov atom. SIGKILL leaves
        // the .mp4 truncated and unplayable.
        //
        // The probe is a bash subprocess with a SIGTERM trap that writes a
        // sentinel file. We use a busy loop (`while :; do :; done`) rather
        // than `sleep`, because bash defers trap handlers until the current
        // external command returns — `sleep` would swallow the signal until
        // it finishes, defeating the test.
        let dir = std::env::temp_dir();
        let sentinel = dir.join(format!(
            "scrcpy-kill-sigterm-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let sentinel_str = sentinel.to_string_lossy().to_string();
        let script = format!(
            "trap 'echo got-term > {s}; exit 0' TERM; while :; do :; done",
            s = sentinel_str
        );
        let child = tokio::process::Command::new("bash")
            .args(["-c", &script])
            .spawn()
            .unwrap();
        // Let the trap install before sending the signal.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(kill(child).await.is_ok());
        // Give the trap a beat to flush the sentinel to disk.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            sentinel.exists(),
            "sentinel file missing at {sentinel_str} — kill() sent SIGKILL instead of SIGTERM"
        );
        let _ = std::fs::remove_file(&sentinel);
    }
}
