// Thin IPC layer over Tauri commands. Components depend on the Backend
// interface, not on @tauri-apps/api directly, so tests inject a fake.

import type { Device, KeyAction, Preset } from "./types";

export interface Backend {
  listDevices(): Promise<Device[]>;
  launchScrcpy(serial: string, preset: Preset): Promise<void>;
  connectWireless(ip: string): Promise<void>;
  adbAvailable(): Promise<boolean>;
  sendKey(action: KeyAction): Promise<void>;
  accessibilityStatus(): Promise<boolean>;
  openAccessibilitySettings(): Promise<void>;
}

// Real backend: lazy-imports @tauri-apps/api so vitest (jsdom) never loads it.
export const tauriBackend: Backend = {
  async listDevices() {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<Device[]>("list_devices");
  },
  async launchScrcpy(serial, preset) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<void>("launch_scrcpy", { serial, args: preset.args });
  },
  async connectWireless(ip) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<void>("connect_wireless", { ip });
  },
  async adbAvailable() {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<boolean>("adb_available");
  },
  async sendKey(action) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<void>("send_key", { action });
  },
  async accessibilityStatus() {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<boolean>("accessibility_status");
  },
  async openAccessibilitySettings() {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<void>("open_accessibility_settings");
  },
};
