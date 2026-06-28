// Unified error model (PRD §4). Serializes with a `kind` tag + optional
// `message` so the TypeScript AppError union (src/types.ts) can discriminate.
//
//   Rust AppError::ScrcpyLaunchFailed("x")
//     ──serde──▶  { "kind": "ScrcpyLaunchFailed", "message": "x" }
//     ──TS─────▶  { kind: "ScrcpyLaunchFailed", message: "x" }

use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    #[error("adb 未找到")]
    AdbNotFound,
    #[error("设备未连接")]
    DeviceNotConnected,
    #[error("scrcpy 启动失败: {0}")]
    ScrcpyLaunchFailed(String),
    #[error("AppleScript 注入失败: {0}")]
    KeyInjectFailed(String),
    #[error("Accessibility 权限被拒")]
    AccessibilityDenied,
    #[error("无线连接失败: {0}")]
    WirelessConnectFailed(String),
    #[error("IO: {0}")]
    Io(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
