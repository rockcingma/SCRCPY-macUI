// ADB integration (PRD §3.3, §5.1). The parsing logic is split from the
// process execution so it can be unit-tested against captured `adb devices`
// output without a real device.
//
//   adb devices -l  ──▶  parse_devices()  ──▶  Vec<Device>
//                                                  │
//                              deriveStatus (TS) ◀─┘  maps to 6-state UI

use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Device {
    pub serial: String,
    pub model: Option<String>,
    #[serde(rename = "rawState")]
    pub raw_state: String,
}

// Candidate adb locations, in priority order (PRD §5.1). Tauri-spawned
// processes don't inherit the shell PATH, so we probe explicit paths.
fn adb_candidates() -> Vec<PathBuf> {
    let mut v = vec![
        PathBuf::from("/opt/homebrew/bin/adb"),
        PathBuf::from("/usr/local/bin/adb"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let mut p = PathBuf::from(home);
        p.push("Library/Android/sdk/platform-tools/adb");
        v.push(p);
    }
    v
}

// First existing adb binary, or None if adb is not installed.
pub fn detect_adb_path() -> Option<PathBuf> {
    adb_candidates().into_iter().find(|p| p.exists())
}

pub fn adb_available() -> bool {
    detect_adb_path().is_some()
}

// Parse `adb devices -l` output into structured Device rows.
// Skips the "List of devices attached" header and blank lines.
//
// Example line:
//   R5CX21RJ6MX  device product:panther model:Pixel_7 device:panther
pub fn parse_devices(output: &str) -> Vec<Device> {
    output
        .lines()
        .skip_while(|l| l.contains("List of devices attached"))
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.contains("List of devices attached"))
        .filter_map(parse_device_line)
        .collect()
}

fn parse_device_line(line: &str) -> Option<Device> {
    let mut parts = line.split_whitespace();
    let serial = parts.next()?.to_string();
    let raw_state = parts.next()?.to_string();
    // model:Pixel_7 → "Pixel 7"
    let model = parts
        .find_map(|p| p.strip_prefix("model:"))
        .map(|m| m.replace('_', " "));
    Some(Device {
        serial,
        model,
        raw_state,
    })
}

// Run `adb devices -l` and parse the result.
pub async fn list_devices() -> AppResult<Vec<Device>> {
    let adb = detect_adb_path().ok_or(AppError::AdbNotFound)?;
    let out = tokio::process::Command::new(adb)
        .args(["devices", "-l"])
        .output()
        .await?;
    if !out.status.success() {
        return Err(AppError::Io(String::from_utf8_lossy(&out.stderr).into_owned()));
    }
    Ok(parse_devices(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_output_yields_no_devices() {
        assert!(parse_devices("List of devices attached\n\n").is_empty());
        assert!(parse_devices("").is_empty());
    }

    #[test]
    fn parses_single_ready_device_with_model() {
        let out = "List of devices attached\nR5CX21RJ6MX device product:panther model:Pixel_7 device:panther\n";
        let d = parse_devices(out);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].serial, "R5CX21RJ6MX");
        assert_eq!(d[0].raw_state, "device");
        assert_eq!(d[0].model.as_deref(), Some("Pixel 7"));
    }

    #[test]
    fn parses_unauthorized_device() {
        let out = "List of devices attached\nABC12345 unauthorized\n";
        let d = parse_devices(out);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].raw_state, "unauthorized");
        assert!(d[0].model.is_none());
    }

    #[test]
    fn parses_offline_device() {
        let out = "List of devices attached\nABC12345 offline\n";
        let d = parse_devices(out);
        assert_eq!(d[0].raw_state, "offline");
    }

    #[test]
    fn parses_multiple_devices() {
        let out = "List of devices attached\n\
                   AAA11111 device model:Pixel_7\n\
                   BBB22222 device model:Galaxy_S23\n";
        let d = parse_devices(out);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].serial, "AAA11111");
        assert_eq!(d[1].model.as_deref(), Some("Galaxy S23"));
    }

    #[test]
    fn ignores_blank_and_header_lines() {
        let out = "List of devices attached\n\n\nR5CX21RJ6MX device\n\n";
        assert_eq!(parse_devices(out).len(), 1);
    }

    #[test]
    fn adb_candidates_includes_homebrew_and_sdk() {
        let c = adb_candidates();
        assert!(c.iter().any(|p| p.ends_with("opt/homebrew/bin/adb")
            || p == &PathBuf::from("/opt/homebrew/bin/adb")));
    }
}
