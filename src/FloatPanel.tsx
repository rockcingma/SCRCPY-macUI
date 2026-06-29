import { useCallback, useEffect, useState } from "react";
import type { Backend } from "./backend";
import { FLOAT_BUTTONS, type AppError, type KeyAction } from "./types";

interface FloatPanelProps {
  backend: Backend;
}

type ButtonFlash = "none" | "press" | "error";

// Float panel — runtime control surface that appears next to scrcpy.
// Drag-region is delegated to Tauri (data-tauri-drag-region), so we only
// handle button presses, press-feedback animation, and error states.
export function FloatPanel({ backend }: FloatPanelProps) {
  const [flash, setFlash] = useState<Record<KeyAction, ButtonFlash>>(() => {
    const init = {} as Record<KeyAction, ButtonFlash>;
    for (const b of FLOAT_BUTTONS) init[b.action] = "none";
    return init;
  });
  const [accessibilityWarning, setAccessibilityWarning] = useState(false);

  const setButtonFlash = useCallback(
    (action: KeyAction, kind: ButtonFlash) => {
      setFlash((prev) => ({ ...prev, [action]: kind }));
    },
    [],
  );

  const press = useCallback(
    async (action: KeyAction) => {
      // PRD §3.5 reflex: 80ms accent outline flash, independent of backend
      // success. This is what tells the user "yes, I registered your click."
      setButtonFlash(action, "press");
      window.setTimeout(() => setButtonFlash(action, "none"), 80);

      try {
        await backend.sendKey(action);
      } catch (e) {
        // 150ms red flash on failure (PRD §3.5).
        setButtonFlash(action, "error");
        window.setTimeout(() => setButtonFlash(action, "none"), 150);
        const err = e as AppError;
        if (err && err.kind === "AccessibilityDenied") {
          setAccessibilityWarning(true);
        }
      }
    },
    [backend, setButtonFlash],
  );

  // Probe accessibility once on mount; surface a banner if denied.
  // Re-probe is wired to the "重试" button below so users can confirm
  // after they flip the System Settings switch without restarting the app.
  const reprobeAccessibility = useCallback(async () => {
    const ok = await backend.accessibilityStatus();
    setAccessibilityWarning(!ok);
  }, [backend]);

  useEffect(() => {
    let cancelled = false;
    void backend.accessibilityStatus().then((ok) => {
      if (!cancelled && !ok) setAccessibilityWarning(true);
    });
    return () => {
      cancelled = true;
    };
  }, [backend]);

  // Drag via the explicit Tauri API. `data-tauri-drag-region` is flaky on
  // focus:false transparent windows, so we call startDragging() directly
  // on mousedown over the handle.
  const startDrag = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().startDragging();
    } catch {
      // No-op in non-Tauri contexts (tests).
    }
  }, []);

  return (
    <div className="float-panel">
      {/* Drag handle. mousedown → Tauri startDragging() moves the window. */}
      <div
        className="drag-handle"
        title="拖动"
        onMouseDown={() => void startDrag()}
      >
        <span aria-hidden>⋮⋮</span>
      </div>
      {accessibilityWarning && (
        <div className="accessibility-banner" role="alert">
          <span>需开启辅助功能</span>
          <button
            type="button"
            className="accessibility-link"
            onClick={() => void backend.openAccessibilitySettings()}
          >
            打开设置
          </button>
          <button
            type="button"
            className="accessibility-link"
            onClick={() => void reprobeAccessibility()}
          >
            已授权,重试
          </button>
        </div>
      )}
      <div className="float-buttons">
        {FLOAT_BUTTONS.map((b) => (
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
      </div>
    </div>
  );
}
