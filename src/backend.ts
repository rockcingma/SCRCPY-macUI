// Thin IPC layer over Tauri commands. Components depend on the Backend
// interface, not on @tauri-apps/api directly, so tests inject a fake.

import type {
  AlwaysOnTopState,
  AudioHostState,
  Device,
  KeyAction,
  Preset,
  RecordDirState,
  RecordingState,
  ScreenOffState,
} from "./types";

export interface Backend {
  listDevices(): Promise<Device[]>;
  launchScrcpy(serial: string, preset: Preset): Promise<void>;
  connectWireless(ip: string): Promise<void>;
  // USB-bootstrapped wireless (method A): switch the device to TCP mode and
  // return its Wi-Fi IP (empty string if it couldn't be read).
  enableTcpip(serial: string): Promise<string>;
  // Wireless pairing (method B, Android 11+). Does NOT connect — follow up
  // with connectWireless on the device's shown connect port.
  pairWireless(ip: string, port: string, code: string): Promise<void>;
  adbAvailable(): Promise<boolean>;
  sendKey(action: KeyAction): Promise<void>;
  toggleRecording(): Promise<RecordingState>;
  toggleScreenOff(): Promise<ScreenOffState>;
  toggleAudioHost(): Promise<AudioHostState>;
  toggleAlwaysOnTop(): Promise<AlwaysOnTopState>;
  // Pass null to reset to the default (~/Desktop). The result tells you what
  // path is actually in effect afterwards.
  setRecordDir(path: string | null): Promise<RecordDirState>;
  // Open the physical keyboard settings on the device (same as MOD+k in scrcpy).
  openKeyboardSettings(): Promise<void>;
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
  async enableTcpip(serial) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<string>("enable_tcpip", { serial });
  },
  async pairWireless(ip, port, code) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<void>("pair_wireless", { ip, port, code });
  },
  async adbAvailable() {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<boolean>("adb_available");
  },
  async sendKey(action) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<void>("send_key", { action });
  },
  async toggleRecording() {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<RecordingState>("toggle_recording");
  },
  async toggleScreenOff() {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<ScreenOffState>("toggle_screen_off");
  },
  async toggleAudioHost() {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<AudioHostState>("toggle_audio_host");
  },
  async toggleAlwaysOnTop() {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<AlwaysOnTopState>("toggle_always_on_top");
  },
  async setRecordDir(path) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<RecordDirState>("set_record_dir", { path });
  },
  async openKeyboardSettings() {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<void>("open_keyboard_settings");
  },
};
