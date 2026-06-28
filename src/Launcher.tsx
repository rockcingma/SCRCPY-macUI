import { useEffect, useState, useCallback } from "react";
import type { Backend } from "./backend";
import type { DeviceState, Preset } from "./types";
import { PRESETS, presetById } from "./types";
import { deriveStatus, stateLabel, stateDot, type DeviceStatus } from "./device";

interface LauncherProps {
  backend: Backend;
  // Last preset id from persisted settings (null on first run).
  lastPresetId: string | null;
  onPresetUsed: (presetId: string) => void;
}

type LaunchPhase = "idle" | "launching" | "error";

export function Launcher({ backend, lastPresetId, onPresetUsed }: LauncherProps) {
  const [status, setStatus] = useState<DeviceStatus>({
    state: "detecting" as DeviceState,
    devices: [],
    active: null,
  });
  const [adbMissing, setAdbMissing] = useState(false);
  const [phase, setPhase] = useState<LaunchPhase>("idle");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const ok = await backend.adbAvailable();
    if (!ok) {
      setAdbMissing(true);
      return;
    }
    setAdbMissing(false);
    const devices = await backend.listDevices();
    setStatus(deriveStatus(devices));
  }, [backend]);

  // Foreground polling at 1Hz (PRD §5.4). The backend pauses itself when
  // the window is hidden, so a steady interval here is safe.
  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), 1000);
    return () => clearInterval(id);
  }, [refresh]);

  const effectiveState: DeviceState = adbMissing ? "adb_missing" : status.state;

  const launch = useCallback(
    async (preset: Preset) => {
      if (!status.active) return;
      setPhase("launching");
      setErrorMsg(null);
      try {
        await backend.launchScrcpy(status.active.serial, preset);
        onPresetUsed(preset.id);
        setPhase("idle");
      } catch (e) {
        // AppError is serde-tagged; surface its message or a fallback.
        const msg =
          e && typeof e === "object" && "message" in e
            ? String((e as { message: unknown }).message)
            : "启动失败";
        setErrorMsg(msg);
        setPhase("error");
      }
    },
    [backend, status.active, onPresetUsed],
  );

  // Primary preset: last-used (PRD D2), falling back to the first preset.
  const primary = (lastPresetId && presetById(lastPresetId)) || PRESETS[0];
  const secondary = PRESETS.filter((p) => p.id !== primary.id);
  const canLaunch = effectiveState === "connected" || effectiveState === "multiple";

  return (
    <div className="launcher">
      <DeviceCard
        state={effectiveState}
        status={status}
        onRefresh={() => void refresh()}
      />

      {errorMsg && (
        <div className="error-bar" role="alert">
          {errorMsg}
          <button onClick={() => status.active && void launch(primary)}>重试</button>
        </div>
      )}

      <button
        className="primary-launch"
        disabled={!canLaunch || phase === "launching"}
        onClick={() => void launch(primary)}
      >
        <span className="primary-label">
          {phase === "launching" ? "启动中..." : primary.label}
        </span>
        <span className="primary-spec">{primary.spec}</span>
      </button>

      <div className="preset-grid" role="group" aria-label="预设">
        {secondary.map((p) => (
          <button
            key={p.id}
            className="preset-mini"
            title={p.label}
            aria-label={p.label}
            disabled={!canLaunch || phase === "launching"}
            onClick={() => void launch(p)}
          >
            {p.label}
          </button>
        ))}
      </div>
    </div>
  );
}

interface DeviceCardProps {
  state: DeviceState;
  status: DeviceStatus;
  onRefresh: () => void;
}

function DeviceCard({ state, status, onRefresh }: DeviceCardProps) {
  return (
    <div className="device-card" data-state={state}>
      <span className={`dot dot-${stateDot(state)}`} aria-hidden />
      <div className="device-info">
        <div className="device-label">
          {status.active?.model ?? stateLabel(state)}
        </div>
        {status.active && (
          <div className="device-serial">{status.active.serial}</div>
        )}
        {state === "multiple" && (
          <div className="device-count">共 {status.devices.length} 台</div>
        )}
        {state === "adb_missing" && (
          <button className="install-adb">安装 adb</button>
        )}
      </div>
      <button className="refresh" onClick={onRefresh} aria-label="刷新设备">
        ↻
      </button>
    </div>
  );
}
