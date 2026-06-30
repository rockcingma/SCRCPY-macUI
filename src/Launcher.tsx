import { useEffect, useState, useCallback, useRef } from "react";
import type { Backend } from "./backend";
import type { DeviceState, Preset } from "./types";
import { PRESETS, presetById } from "./types";
import { deriveStatus, stateLabel, stateDot, type DeviceStatus } from "./device";

interface LauncherProps {
  backend: Backend;
  // Last preset id from persisted settings (null on first run).
  lastPresetId: string | null;
  // Persisted recording destination. null = backend default (~/Desktop).
  recordDir: string | null;
  // Recently-connected wireless addresses (most-recent first).
  ipHistory: string[];
  onPresetUsed: (presetId: string) => void;
  onRecordDirChanged: (dir: string | null) => void;
  // Called with a freshly-connected ip:port so the parent can persist history.
  onIpUsed: (ip: string) => void;
}

type LaunchPhase = "idle" | "launching" | "error";

export function Launcher({
  backend,
  lastPresetId,
  recordDir,
  ipHistory,
  onPresetUsed,
  onRecordDirChanged,
  onIpUsed,
}: LauncherProps) {
  const [status, setStatus] = useState<DeviceStatus>({
    state: "detecting" as DeviceState,
    devices: [],
    active: null,
  });
  const [adbMissing, setAdbMissing] = useState(false);
  const [phase, setPhase] = useState<LaunchPhase>("idle");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  // Display copy for the record-dir row. `effective` is what the backend will
  // actually write to (the persisted choice, or its default). `warning` shows
  // briefly when the user picked an unwritable folder and we fell back.
  const [recordEffective, setRecordEffective] = useState<string | null>(recordDir);
  const [recordWarning, setRecordWarning] = useState<string | null>(null);

  // ── Wireless connection (collapsible, low-frequency) ──────────────────
  const [wirelessOpen, setWirelessOpen] = useState(false);
  // Shared "working / error / hint" line for the whole wireless section.
  const [wirelessMsg, setWirelessMsg] = useState<string | null>(null);
  const [wirelessBusy, setWirelessBusy] = useState(false);
  // Method A (USB-bootstrapped): the connect address, pre-filled by tcpip.
  const [connectIp, setConnectIp] = useState("");
  // Method B (pairing): host, pairing port, 6-digit code.
  const [pairIp, setPairIp] = useState("");
  const [pairPort, setPairPort] = useState("");
  const [pairCode, setPairCode] = useState("");

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

  // 手动选择设备（多设备场景下）
  const selectDevice = useCallback((serial: string) => {
    setStatus((prev) => {
      const selected = prev.devices.find((d) => d.serial === serial);
      if (!selected) return prev;
      return { ...prev, active: selected };
    });
  }, []);

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

  const chooseRecordDir = useCallback(async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({ directory: true, multiple: false });
      // open() returns null when the user cancels — leave settings untouched.
      if (typeof picked !== "string") return;
      const result = await backend.setRecordDir(picked);
      setRecordEffective(result.effective);
      if (result.accepted) {
        setRecordWarning(null);
        // Persist the user's choice. If the backend rejected it we DON'T
        // persist — the next launch would otherwise re-replay an invalid path.
        onRecordDirChanged(picked);
      } else {
        setRecordWarning(result.message ?? "目录不可用，已退回上一个设置");
      }
    } catch (e) {
      setRecordWarning(e instanceof Error ? e.message : String(e));
    }
  }, [backend, onRecordDirChanged]);

  const resetRecordDir = useCallback(async () => {
    try {
      const result = await backend.setRecordDir(null);
      setRecordEffective(null); // show "默认" in the UI
      setRecordWarning(null);
      onRecordDirChanged(null);
      // Refresh the displayed path from the backend in case the user wants
      // to see where the default actually lives.
      setRecordEffective(result.effective);
    } catch {
      // Non-fatal; UI keeps showing the prior value.
    }
  }, [backend, onRecordDirChanged]);

  // Surface a backend AppError's message, or a fallback. Shared by the
  // wireless handlers since they all reject with the serde-tagged union.
  const errText = (e: unknown, fallback: string) =>
    e && typeof e === "object" && "message" in e
      ? String((e as { message: unknown }).message)
      : fallback;

  // Method A step 1: flip the USB device into TCP mode, pre-fill its IP.
  const enableTcpip = useCallback(async () => {
    if (!status.active) return;
    setWirelessBusy(true);
    setWirelessMsg("正在切换到无线模式...");
    try {
      const ip = await backend.enableTcpip(status.active.serial);
      if (ip) {
        setConnectIp(`${ip}:5555`);
        setWirelessMsg("已切换。可拔掉数据线，然后点连接。");
      } else {
        setWirelessMsg("已切换到无线模式。请手动填写设备 IP 后连接。");
      }
    } catch (e) {
      setWirelessMsg(errText(e, "切换失败"));
    } finally {
      setWirelessBusy(false);
    }
  }, [backend, status.active]);

  // Connect to an address (shared by method A, history re-connect). On success
  // we persist it to history and refresh the device list.
  const connect = useCallback(
    async (ip: string) => {
      const target = ip.trim();
      if (!target) return;
      setWirelessBusy(true);
      setWirelessMsg(`正在连接 ${target}...`);
      try {
        await backend.connectWireless(target);
        onIpUsed(target);
        setWirelessMsg(`已连接 ${target}`);
        await refresh();
      } catch (e) {
        setWirelessMsg(errText(e, "连接失败"));
      } finally {
        setWirelessBusy(false);
      }
    },
    [backend, onIpUsed, refresh],
  );

  // Method B: pair (Android 11+), then the user connects via the connect port.
  const pair = useCallback(async () => {
    setWirelessBusy(true);
    setWirelessMsg("正在配对...");
    try {
      await backend.pairWireless(pairIp.trim(), pairPort.trim(), pairCode.trim());
      setWirelessMsg("配对成功。请在上方填写连接地址 (IP:5555) 并连接。");
      // Pre-fill the connect field with the paired host for convenience.
      const host = pairIp.trim().split(":")[0];
      if (host) setConnectIp(`${host}:5555`);
    } catch (e) {
      setWirelessMsg(errText(e, "配对失败"));
    } finally {
      setWirelessBusy(false);
    }
  }, [backend, pairIp, pairPort, pairCode]);

  return (
    <div className="launcher">
      <DeviceCard
        state={effectiveState}
        status={status}
        onRefresh={() => void refresh()}
        onSelectDevice={selectDevice}
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

      <div className="record-dir-row" role="group" aria-label="录屏保存位置">
        <span className="record-dir-label">录屏保存到</span>
        <span
          className="record-dir-path"
          title={recordEffective ?? "默认 (~/Desktop)"}
        >
          {recordEffective ?? "默认 (~/Desktop)"}
        </span>
        <button
          type="button"
          className="record-dir-button"
          onClick={() => void chooseRecordDir()}
        >
          更改...
        </button>
        {recordEffective && (
          <button
            type="button"
            className="record-dir-reset"
            onClick={() => void resetRecordDir()}
            aria-label="恢复默认位置"
            title="恢复默认位置"
          >
            ↺
          </button>
        )}
      </div>
      {recordWarning && (
        <div className="record-dir-warning" role="status">
          {recordWarning}
        </div>
      )}

      <div className="keyboard-hint" role="note">
        <span className="hint-icon">⌨️</span>
        <div className="hint-content">
          <div className="hint-title">Mac 键盘输入使用说明（UHID 物理键盘模式）</div>
          <ol className="hint-steps">
            <li><strong>首次配置</strong>（只需一次）：投屏后按 <kbd>⌘K</kbd>，手机打开物理键盘设置，点击激活任一布局</li>
            <li><strong>日常使用</strong>：Mac 切到 <strong>ABC 英文</strong>输入法（Ctrl+Space）</li>
            <li>点击 scrcpy 窗口确保焦点，直接在 Mac 键盘打拼音</li>
            <li>手机输入法（搜狗/三星）会显示候选，按空格/数字选词</li>
          </ol>
          <div className="hint-note">
            💡 <strong>原理：</strong>UHID 模式模拟 USB 物理键盘，手机系统识别为外接键盘，虚拟键盘自动退让。配置一次后永久生效。<br/>
            ⚠️ <strong>注意：</strong>Mac 必须在英文输入法下，拼音由手机输入法处理。虚拟键盘可以按返回键隐藏。
          </div>
        </div>
      </div>

      <div className="wireless-section">
        <button
          type="button"
          className="wireless-toggle"
          aria-expanded={wirelessOpen}
          onClick={() => setWirelessOpen((v) => !v)}
        >
          <span>无线连接</span>
          <span aria-hidden>{wirelessOpen ? "▾" : "▸"}</span>
        </button>
        {/* WIRELESS_BODY_PLACEHOLDER */}
        {wirelessOpen && (
          <div className="wireless-body">
            {/* History: quick re-connect to a previously used address. */}
            {ipHistory.length > 0 && (
              <div className="wireless-history">
                <span className="wireless-label">历史地址</span>
                {ipHistory.map((ip) => (
                  <button
                    key={ip}
                    type="button"
                    className="wireless-history-item"
                    disabled={wirelessBusy}
                    onClick={() => void connect(ip)}
                  >
                    {ip}
                  </button>
                ))}
              </div>
            )}

            {/* Method A — USB-bootstrapped. */}
            <div className="wireless-method">
              <div className="wireless-method-title">方式 A · 数据线引导</div>
              <p className="wireless-hint">
                首次需用数据线连接，点下方按钮切到无线，再拔线连接。
              </p>
              <button
                type="button"
                className="wireless-btn"
                disabled={wirelessBusy || !status.active}
                onClick={() => void enableTcpip()}
              >
                切换到无线模式
              </button>
              <div className="wireless-row">
                <input
                  className="wireless-input"
                  type="text"
                  placeholder="192.168.x.x:5555"
                  value={connectIp}
                  disabled={wirelessBusy}
                  onChange={(e) => setConnectIp(e.target.value)}
                />
                <button
                  type="button"
                  className="wireless-btn"
                  disabled={wirelessBusy || connectIp.trim() === ""}
                  onClick={() => void connect(connectIp)}
                >
                  连接
                </button>
              </div>
            </div>

            {/* Method B — wireless pairing (Android 11+). */}
            <div className="wireless-method">
              <div className="wireless-method-title">方式 B · 无线配对 (Android 11+)</div>
              <p className="wireless-hint">
                手机「开发者选项 → 无线调试 → 使用配对码配对」读取 IP、端口、配对码。
              </p>
              <div className="wireless-row">
                <input
                  className="wireless-input"
                  type="text"
                  placeholder="配对 IP"
                  value={pairIp}
                  disabled={wirelessBusy}
                  onChange={(e) => setPairIp(e.target.value)}
                />
                <input
                  className="wireless-input wireless-input-sm"
                  type="text"
                  placeholder="端口"
                  value={pairPort}
                  disabled={wirelessBusy}
                  onChange={(e) => setPairPort(e.target.value)}
                />
              </div>
              <div className="wireless-row">
                <input
                  className="wireless-input"
                  type="text"
                  placeholder="6 位配对码"
                  value={pairCode}
                  disabled={wirelessBusy}
                  onChange={(e) => setPairCode(e.target.value)}
                />
                <button
                  type="button"
                  className="wireless-btn"
                  disabled={
                    wirelessBusy ||
                    pairIp.trim() === "" ||
                    pairPort.trim() === "" ||
                    pairCode.trim() === ""
                  }
                  onClick={() => void pair()}
                >
                  配对
                </button>
              </div>
            </div>

            {wirelessMsg && (
              <div className="wireless-msg" role="status">
                {wirelessMsg}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

interface DeviceCardProps {
  state: DeviceState;
  status: DeviceStatus;
  onRefresh: () => void;
  onSelectDevice?: (serial: string) => void;
}

function DeviceCard({ state, status, onRefresh, onSelectDevice }: DeviceCardProps) {
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  const handleSelectDevice = (serial: string) => {
    onSelectDevice?.(serial);
    setDropdownOpen(false);
  };

  // 点击外部关闭下拉列表
  useEffect(() => {
    if (!dropdownOpen) return;

    const handleClickOutside = (event: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setDropdownOpen(false);
      }
    };

    // 延迟添加监听器，避免立即触发
    const timer = setTimeout(() => {
      document.addEventListener("mousedown", handleClickOutside);
    }, 0);

    return () => {
      clearTimeout(timer);
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [dropdownOpen]);

  // 当只有一台设备或状态不是 multiple 时，显示简单视图
  if (state !== "multiple" || status.devices.length <= 1) {
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

  // 多设备选择器
  return (
    <div className="device-card device-card-with-selector" data-state={state}>
      <span className={`dot dot-${stateDot(state)}`} aria-hidden />
      <div className="device-selector" ref={dropdownRef}>
        <button
          className="device-trigger"
          type="button"
          aria-haspopup="listbox"
          aria-expanded={dropdownOpen}
          onClick={() => setDropdownOpen(!dropdownOpen)}
        >
          <div className="device-trigger-content">
            <div className="device-name">{status.active?.model ?? "选择设备"}</div>
            <div className="device-meta">
              <span className="device-serial">{status.active?.serial ?? ""}</span>
              <span className="device-count">共 {status.devices.length} 台</span>
            </div>
          </div>
          <span className={`device-trigger-icon ${dropdownOpen ? "open" : ""}`}>▾</span>
        </button>

        {dropdownOpen && (
          <div className="device-dropdown" role="listbox">
            <ul className="device-list">
              {status.devices.map((device) => (
                <li
                  key={device.serial}
                  className={`device-item ${
                    status.active?.serial === device.serial ? "selected" : ""
                  }`}
                  role="option"
                  aria-selected={status.active?.serial === device.serial}
                  onClick={() => handleSelectDevice(device.serial)}
                >
                  <div className="device-item-indicator" />
                  <div className="device-item-info">
                    <div className="device-item-name">{device.model}</div>
                    <div className="device-item-serial">{device.serial}</div>
                  </div>
                  <div className="device-item-checkmark" />
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
      <button className="refresh" onClick={onRefresh} aria-label="刷新设备">
        ↻
      </button>
    </div>
  );
}
