import { useEffect, useState } from "react";
import { Launcher } from "./Launcher";
import { FloatPanel } from "./FloatPanel";
import { tauriBackend } from "./backend";
import { coerceSettings, pushIpHistory, type Settings, DEFAULT_SETTINGS } from "./store/settings";

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
        const store = await load("settings.json", { defaults: {}, autoSave: true });
        const raw = await store.get("settings");
        if (cancelled) return;
        const coerced = coerceSettings(raw);
        setSettings(coerced);
        // Replay the persisted record-dir into the backend on startup so the
        // recording command sees the right destination from the first click.
        // We don't wait on / surface the result here — if it's invalid (e.g.
        // the user deleted the folder), the next recording attempt will tell
        // them, and the Launcher's path display will refresh.
        if (coerced.recordDir) {
          void tauriBackend.setRecordDir(coerced.recordDir).catch(() => {});
        }
      } catch {
        if (!cancelled) setSettings({ ...DEFAULT_SETTINGS });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const persist = async (next: Settings) => {
    setSettings(next);
    try {
      const { load } = await import("@tauri-apps/plugin-store");
      const store = await load("settings.json", { defaults: {}, autoSave: true });
      await store.set("settings", next);
    } catch {
      // Non-fatal: persistence best-effort.
    }
  };

  const onPresetUsed = (presetId: string) =>
    void persist({ ...(settings ?? DEFAULT_SETTINGS), lastPresetId: presetId });

  const onRecordDirChanged = (dir: string | null) =>
    void persist({ ...(settings ?? DEFAULT_SETTINGS), recordDir: dir });

  const onIpUsed = (ip: string) => {
    const prev = settings ?? DEFAULT_SETTINGS;
    void persist({ ...prev, ipHistory: pushIpHistory(prev.ipHistory, ip) });
  };

  if (!settings) return <div className="loading">正在加载...</div>;

  return (
    <Launcher
      backend={tauriBackend}
      lastPresetId={settings.lastPresetId}
      recordDir={settings.recordDir}
      ipHistory={settings.ipHistory}
      onPresetUsed={onPresetUsed}
      onRecordDirChanged={onRecordDirChanged}
      onIpUsed={onIpUsed}
    />
  );
}
