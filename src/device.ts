// Pure device-state derivation — no Tauri imports, fully unit-testable.
// Maps a raw device list (from adb.rs) into the 6-state UI model (PRD §3.3).

import type { Device, DeviceState } from "./types";

export interface DeviceStatus {
  state: DeviceState;
  devices: Device[];
  // The device the UI should act on (first "device"-state entry, or null).
  active: Device | null;
}

// adbMissing is signalled separately by the backend (AdbNotFound error),
// so this function only handles the device-list cases.
export function deriveStatus(devices: Device[]): DeviceStatus {
  const ready = devices.filter((d) => d.rawState === "device");
  const unauthorized = devices.filter((d) => d.rawState === "unauthorized");

  if (devices.length === 0) {
    return { state: "empty", devices, active: null };
  }
  // Any device awaiting authorization, and none ready → surface unauthorized.
  if (ready.length === 0 && unauthorized.length > 0) {
    return { state: "unauthorized", devices, active: null };
  }
  if (ready.length === 0) {
    // Devices present but all offline/unknown → treat as empty-actionable.
    return { state: "empty", devices, active: null };
  }
  if (ready.length === 1) {
    return { state: "connected", devices, active: ready[0] };
  }
  return { state: "multiple", devices, active: ready[0] };
}

// Human label for each state (PRD §3.3 table).
export function stateLabel(state: DeviceState): string {
  switch (state) {
    case "detecting":
      return "正在检测设备...";
    case "empty":
      return "未检测到设备";
    case "unauthorized":
      return "设备已连接，等待授权";
    case "adb_missing":
      return "未找到 adb";
    case "connected":
      return "已连接";
    case "multiple":
      return "检测到多台设备";
  }
}

// Status dot color token per state.
export function stateDot(state: DeviceState): string {
  switch (state) {
    case "detecting":
    case "empty":
      return "gray";
    case "unauthorized":
      return "yellow";
    case "adb_missing":
      return "orange";
    case "connected":
      return "green";
    case "multiple":
      return "blue";
  }
}
