// Shared domain types mirroring the Rust backend (src-tauri/src/adb.rs, error.rs).
// Keep these in sync with the serde-tagged AppError and Device structs.

export type DeviceState =
  | "detecting" // 🔄 polling adb
  | "empty" // ⚫ no device
  | "unauthorized" // 🟡 connected, awaiting authorization
  | "adb_missing" // 🟠 adb binary not found
  | "connected" // 🟢 one device ready
  | "multiple"; // 🔵 more than one device

export interface Device {
  serial: string;
  model: string | null;
  // "device" | "unauthorized" | "offline" — raw adb state for this entry.
  rawState: string;
}

// Discriminated union mirroring Rust AppError #[serde(tag = "kind", content = "message")].
export type AppError =
  | { kind: "AdbNotFound" }
  | { kind: "DeviceNotConnected" }
  | { kind: "ScrcpyLaunchFailed"; message: string }
  | { kind: "KeyInjectFailed"; message: string }
  | { kind: "AccessibilityDenied" }
  | { kind: "WirelessConnectFailed"; message: string }
  | { kind: "Io"; message: string };

export function errorToMessage(e: AppError): string {
  switch (e.kind) {
    case "AdbNotFound":
      return "未找到 adb";
    case "DeviceNotConnected":
      return "设备未连接";
    case "ScrcpyLaunchFailed":
      return `启动失败：${e.message}`;
    case "KeyInjectFailed":
      return `按键注入失败：${e.message}`;
    case "AccessibilityDenied":
      return "需要辅助功能权限";
    case "WirelessConnectFailed":
      return humanizeWirelessError(e.message);
    case "Io":
      return `IO 错误：${e.message}`;
  }
}

// Map raw adb/scrcpy stderr into plain-language guidance (PRD §3.3).
export function humanizeWirelessError(raw: string): string {
  if (/failed to connect|connection refused/i.test(raw)) {
    return "目标设备未开启 5555 端口，请先用 USB 线执行 adb tcpip 5555";
  }
  if (/timeout|timed out/i.test(raw)) {
    return "连接超时，请确认设备与电脑在同一网络";
  }
  return `连接失败：${raw}`;
}

export interface Preset {
  id: string;
  label: string;
  // SF Symbol name, rendered as inline SVG by the icon layer.
  icon: string;
  // Short spec line shown under the primary button.
  spec: string;
  // scrcpy args, passed as argv (never shell-interpolated).
  args: string[];
}

export const PRESETS: Preset[] = [
  {
    id: "high-quality",
    label: "高画质启动",
    icon: "bolt.fill",
    spec: "1920px · 8M · 60fps",
    args: ["--max-size=1920", "--video-bit-rate=8M", "--max-fps=60"],
  },
  {
    id: "wifi-balanced",
    label: "WiFi 均衡",
    icon: "wifi",
    spec: "1280px · 4M · 30fps",
    args: ["--max-size=1280", "--video-bit-rate=4M", "--max-fps=30"],
  },
  {
    id: "game-low-latency",
    label: "游戏低延迟",
    icon: "gamecontroller.fill",
    spec: "1280px · 4M · 低延迟",
    args: ["--max-size=1280", "--video-bit-rate=4M", "--max-fps=60", "--no-audio"],
  },
  {
    id: "power-save",
    label: "省电",
    icon: "leaf.fill",
    spec: "1024px · 2M · 30fps",
    args: ["--max-size=1024", "--video-bit-rate=2M", "--max-fps=30"],
  },
  {
    id: "demo-readonly",
    label: "演示只读",
    icon: "eye.fill",
    spec: "只读 · 不可控",
    args: ["--no-control"],
  },
  {
    id: "record",
    label: "录屏",
    icon: "record.circle",
    spec: "录制到桌面",
    args: [], // record path injected at launch time
  },
];

export function presetById(id: string): Preset | undefined {
  return PRESETS.find((p) => p.id === id);
}
