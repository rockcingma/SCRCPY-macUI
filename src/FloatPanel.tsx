import { useCallback, useEffect, useState } from "react";
import type { Backend } from "./backend";
import { FLOAT_BUTTONS, type KeyAction } from "./types";

interface FloatPanelProps {
  backend: Backend;
}

type ButtonFlash = "none" | "press" | "error";

// Float panel — runtime control surface that appears beside scrcpy.
// Nav buttons route to `adb shell input keyevent`; recording, screen-off and
// audio-host are stateful toggles (ARCHITECTURE §2) that relaunch scrcpy with
// extra flags appended (--record / --turn-screen-off+--stay-awake / --no-audio).
//
// Layout:
//   drag handle → screen toggle → audio toggle → divider
//   nav buttons (FLOAT_BUTTONS minus close)
//   record toggle → close button
//
// Record sits just above close because it's not a high-frequency action and
// the user asked for it to be the second-to-last button.
export function FloatPanel({ backend }: FloatPanelProps) {
  const [flash, setFlash] = useState<Record<KeyAction, ButtonFlash>>(() => {
    const init = {} as Record<KeyAction, ButtonFlash>;
    for (const b of FLOAT_BUTTONS) init[b.action] = "none";
    return init;
  });
  const [recording, setRecording] = useState(false);
  const [recordBusy, setRecordBusy] = useState(false);
  // Brief "saved to …" toast after a recording stops.
  const [savedToast, setSavedToast] = useState<string | null>(null);
  // Phone-screen-off toggle. Independent of recording — both can be on at
  // once, the backend rebuilds argv from base_args + every active flag.
  const [screenOff, setScreenOff] = useState(false);
  const [screenBusy, setScreenBusy] = useState(false);
  // Audio routing: true = Mac plays (scrcpy default), false = device plays.
  const [hostAudio, setHostAudio] = useState(true);
  const [audioBusy, setAudioBusy] = useState(false);
  // Always-on-top toggle: true = scrcpy window stays on top of all others.
  const [alwaysOnTop, setAlwaysOnTop] = useState(false);
  const [pinBusy, setPinBusy] = useState(false);

  const setButtonFlash = useCallback((action: KeyAction, kind: ButtonFlash) => {
    setFlash((prev) => ({ ...prev, [action]: kind }));
  }, []);

  const press = useCallback(
    async (action: KeyAction) => {
      // 80ms accent flash, independent of backend result (PRD §3.5).
      setButtonFlash(action, "press");
      window.setTimeout(() => setButtonFlash(action, "none"), 80);
      try {
        await backend.sendKey(action);
      } catch {
        setButtonFlash(action, "error");
        window.setTimeout(() => setButtonFlash(action, "none"), 150);
      }
    },
    [backend, setButtonFlash],
  );

  const toggleRecording = useCallback(async () => {
    if (recordBusy) return; // relaunch takes a moment; ignore double-clicks
    setRecordBusy(true);
    try {
      const state = await backend.toggleRecording();
      setRecording(state.recording);
      if (!state.recording && state.savedPath) {
        // Show just the filename — the panel is narrow.
        const name = state.savedPath.split("/").pop() ?? state.savedPath;
        setSavedToast(`已保存 ${name}`);
        window.setTimeout(() => setSavedToast(null), 3000);
      }
    } catch {
      // Leave state unchanged on failure; the user can retry.
    } finally {
      setRecordBusy(false);
    }
  }, [backend, recordBusy]);

  const toggleScreenOff = useCallback(async () => {
    // Each toggle relaunches scrcpy — ignore re-entrant clicks while the
    // backend is still spinning up, same as the recording button.
    if (screenBusy) return;
    setScreenBusy(true);
    try {
      const state = await backend.toggleScreenOff();
      setScreenOff(state.screenOff);
    } catch {
      // Leave state unchanged on failure; the user can retry.
    } finally {
      setScreenBusy(false);
    }
  }, [backend, screenBusy]);

  const toggleAudioHost = useCallback(async () => {
    if (audioBusy) return;
    setAudioBusy(true);
    try {
      const state = await backend.toggleAudioHost();
      setHostAudio(state.hostAudio);
    } catch {
      // Same as the other stateful toggles — silently leave state unchanged.
    } finally {
      setAudioBusy(false);
    }
  }, [backend, audioBusy]);

  const toggleAlwaysOnTop = useCallback(async () => {
    if (pinBusy) return;
    setPinBusy(true);
    try {
      const state = await backend.toggleAlwaysOnTop();
      setAlwaysOnTop(state.alwaysOnTop);
    } catch {
      // Silently leave state unchanged on failure.
    } finally {
      setPinBusy(false);
    }
  }, [backend, pinBusy]);

  // Cmd+O shortcut. Scoped to this webview's keydown (not a system-global
  // shortcut) so the rest of macOS still owns Cmd+O — it only fires when our
  // float or main window has focus. preventDefault stops the webview's
  // built-in "open file" default. The panel only mounts while scrcpy is
  // running, so the listener naturally has the right lifetime.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey && e.key.toLowerCase() === "o") {
        e.preventDefault();
        void toggleScreenOff();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [toggleScreenOff]);

  const startDrag = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().startDragging();
    } catch {
      // No-op in non-Tauri contexts (tests).
    }
  }, []);

  // Split nav buttons: the close button is rendered separately at the bottom
  // so the record toggle can sit just above it (user-requested order).
  const navButtons = FLOAT_BUTTONS.filter((b) => b.action !== "close");
  const closeButton = FLOAT_BUTTONS.find((b) => b.action === "close");

  return (
    <div className="float-panel">
      <div
        className="drag-handle"
        title="拖动"
        onMouseDown={() => void startDrag()}
      >
        <span aria-hidden>⋮⋮</span>
      </div>

      {/* Phone-screen toggle — Cmd+O also triggers this. ◐ = phone screen on
          (mirror + physical display lit), ○ = physical display dark. */}
      <button
        className={`screen-button ${screenOff ? "off" : ""}`}
        aria-label={screenOff ? "开启手机屏幕" : "关闭手机屏幕"}
        aria-pressed={screenOff}
        data-tooltip={screenOff ? "开启手机屏幕 (⌘O)" : "关闭手机屏幕 (⌘O)"}
        disabled={screenBusy}
        onClick={() => void toggleScreenOff()}
      >
        <span className="screen-icon" aria-hidden>
          {screenOff ? "○" : "◐"}
        </span>
      </button>

      {/* Audio routing — ♪ = Mac plays, S = device speakers. Stateful toggle. */}
      <button
        className={`audio-button ${hostAudio ? "" : "device"}`}
        aria-label={hostAudio ? "切换到手机播放" : "切换到 Mac 播放"}
        aria-pressed={!hostAudio}
        data-tooltip={hostAudio ? "音频：Mac 播放" : "音频：手机播放"}
        disabled={audioBusy}
        onClick={() => void toggleAudioHost()}
      >
        <span className="audio-icon" aria-hidden>
          {hostAudio ? "♪" : "S"}
        </span>
      </button>

      {/* Always-on-top toggle — pin icon: tilted outline (unpinned) → upright solid (pinned). */}
      <button
        className={`pin-button ${alwaysOnTop ? "pinned" : ""}`}
        aria-label={alwaysOnTop ? "取消固定" : "固定窗口"}
        aria-pressed={alwaysOnTop}
        data-tooltip={alwaysOnTop ? "取消固定" : "固定窗口在最前"}
        disabled={pinBusy}
        onClick={() => void toggleAlwaysOnTop()}
      >
        <svg
          className="pin-icon"
          width="16"
          height="16"
          viewBox="0 0 16 16"
          fill="none"
          xmlns="http://www.w3.org/2000/svg"
        >
          {alwaysOnTop ? (
            // Pinned: upright solid pin (vertical, filled)
            <path
              d="M8 2 L9.5 6 L11 6 L11 10 L9 10 L9 14 L7 14 L7 10 L5 10 L5 6 L6.5 6 Z"
              fill="currentColor"
            />
          ) : (
            // Unpinned: tilted outline pin (rotated, stroked)
            <g transform="rotate(-45 8 8)">
              <path
                d="M8 2 L9.5 6 L11 6 L11 10 L9 10 L9 14 L7 14 L7 10 L5 10 L5 6 L6.5 6 Z"
                stroke="currentColor"
                strokeWidth="1.2"
                fill="none"
              />
            </g>
          )}
        </svg>
      </button>

      <div className="float-divider" aria-hidden />

      <div className="float-buttons">
        {navButtons.map((b) => (
          <button
            key={b.action}
            className={`float-button flash-${flash[b.action]}`}
            aria-label={b.label}
            data-tooltip={b.label}
            onClick={() => void press(b.action)}
          >
            <span className="float-icon">{b.icon}</span>
          </button>
        ))}

        {/* Recording toggle — second-to-last, low-frequency action. */}
        <button
          className={`record-button ${recording ? "recording" : ""}`}
          aria-label={recording ? "停止录制" : "开始录制"}
          aria-pressed={recording}
          data-tooltip={recording ? "停止录制" : "开始录制"}
          disabled={recordBusy}
          onClick={() => void toggleRecording()}
        >
          <span className="record-icon" aria-hidden>
            {recording ? "■" : "●"}
          </span>
        </button>

        {/* Close mirroring — terminal action, lives at the very bottom. */}
        {closeButton && (
          <button
            key={closeButton.action}
            className={`float-button flash-${flash[closeButton.action]}`}
            aria-label={closeButton.label}
            data-tooltip={closeButton.label}
            onClick={() => void press(closeButton.action)}
          >
            <span className="float-icon">{closeButton.icon}</span>
          </button>
        )}
      </div>

      {savedToast && (
        <div className="record-toast" role="status">
          {savedToast}
        </div>
      )}
    </div>
  );
}
