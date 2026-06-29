import { useEffect, useState } from "react";
import { Launcher } from "./Launcher";
import { FloatPanel } from "./FloatPanel";
import { tauriBackend } from "./backend";
import { coerceSettings, type Settings, DEFAULT_SETTINGS } from "./store/settings";

// Two views live in the same bundle, picked by location hash:
//   #/      → Launcher (main window)
//   #float  → FloatPanel (runtime overlay window)
// tauri.conf.json points the float window at index.html#float, so this
// branches cleanly without React Router weight.
export function App() {
  const isFloat =
    typeof window !== "undefined" && window.location.hash.includes("float");
  return isFloat ? <FloatPanel backend={tauriBackend} /> : <MainView />;
}

function MainView() {
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const { load } = await import("@tauri-apps/plugin-store");
        const store = await load("settings.json", { autoSave: true });
        const raw = await store.get("settings");
        if (!cancelled) setSettings(coerceSettings(raw));
      } catch {
        if (!cancelled) setSettings({ ...DEFAULT_SETTINGS });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // Bridge scrcpy lifecycle events from the backend to the float window.
  // The main window owns the bridge because the float window starts hidden
  // and may not have JS running when the first event fires.
  useEffect(() => {
    let unsubStarted: (() => void) | undefined;
    let unsubStopped: (() => void) | undefined;
    void (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        const { getAllWebviewWindows } = await import(
          "@tauri-apps/api/webviewWindow"
        );
        const { LogicalPosition } = await import("@tauri-apps/api/window");
        const findFloat = async () =>
          (await getAllWebviewWindows()).find((w) => w.label === "float");

        // Position the float window at the right edge of the monitor it
        // lands on, vertically centered (PRD §3.2). Without this, Tauri
        // drops it in the screen center where it covers content.
        const positionToRightEdge = async (w: Awaited<ReturnType<typeof findFloat>>) => {
          if (!w) return;
          const monitor = await w.currentMonitor();
          if (!monitor) return;
          // size is in physical pixels — divide by scale to get logical.
          const scale = monitor.scaleFactor;
          const screenWidth = monitor.size.width / scale;
          const screenHeight = monitor.size.height / scale;
          // Window is 200 wide; visible content (.float-panel) is 56,
          // anchored to the window's right edge. Position the window so
          // its right edge sits 16px in from the screen edge.
          const x = screenWidth - 200 - 16;
          const y = Math.max(40, (screenHeight - 560) / 2);
          await w.setPosition(new LogicalPosition(x, y));
        };

        unsubStarted = await listen("scrcpy-started", async () => {
          const w = await findFloat();
          if (!w) return;
          // Show FIRST — this is the core behavior. Positioning is a
          // nice-to-have; if currentMonitor/setPosition throws (missing
          // capability, null monitor), we must not let it block the show.
          await w.show();
          await w.setAlwaysOnTop(true);
          try {
            await positionToRightEdge(w);
          } catch {
            // Leave the window at its default position.
          }
        });
        unsubStopped = await listen("scrcpy-stopped", async () => {
          const w = await findFloat();
          if (!w) return;
          await w.hide();
        });
      } catch {
        // Non-fatal: in vitest/jsdom this branch never runs.
      }
    })();
    return () => {
      unsubStarted?.();
      unsubStopped?.();
    };
  }, []);

  const onPresetUsed = async (presetId: string) => {
    setSettings((prev) => ({
      ...(prev ?? DEFAULT_SETTINGS),
      lastPresetId: presetId,
    }));
    try {
      const { load } = await import("@tauri-apps/plugin-store");
      const store = await load("settings.json", { autoSave: true });
      const next = { ...(settings ?? DEFAULT_SETTINGS), lastPresetId: presetId };
      await store.set("settings", next);
    } catch {
      // Non-fatal: persistence best-effort.
    }
  };

  if (!settings) return <div className="loading">正在加载...</div>;

  return (
    <Launcher
      backend={tauriBackend}
      lastPresetId={settings.lastPresetId}
      onPresetUsed={(id) => void onPresetUsed(id)}
    />
  );
}
