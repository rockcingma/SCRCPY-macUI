import { useEffect, useState } from "react";
import { Launcher } from "./Launcher";
import { tauriBackend } from "./backend";
import { coerceSettings, type Settings, DEFAULT_SETTINGS } from "./store/settings";

// Loads persisted settings (tauri-plugin-store) then renders the Launcher.
// Store access is isolated here so Launcher stays pure/testable.
export function App() {
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
        // Store unavailable (e.g. first run) → defaults.
        if (!cancelled) setSettings({ ...DEFAULT_SETTINGS });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const onPresetUsed = async (presetId: string) => {
    setSettings((prev) => ({ ...(prev ?? DEFAULT_SETTINGS), lastPresetId: presetId }));
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
