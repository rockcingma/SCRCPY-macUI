import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Launcher } from "../Launcher";
import type { Backend } from "../backend";
import type { Device, Preset } from "../types";

function fakeBackend(overrides: Partial<Backend> = {}): Backend {
  return {
    adbAvailable: vi.fn(async () => true),
    listDevices: vi.fn(async () => [] as Device[]),
    launchScrcpy: vi.fn(async () => {}),
    connectWireless: vi.fn(async () => {}),
    sendKey: vi.fn(async () => {}),
    toggleRecording: vi.fn(async () => ({ recording: false, savedPath: null })),
    toggleScreenOff: vi.fn(async () => ({ screenOff: false })),
    toggleAudioHost: vi.fn(async () => ({ hostAudio: true })),
    toggleAlwaysOnTop: vi.fn(async () => ({ alwaysOnTop: false })),
    enableTcpip: vi.fn(async () => "192.168.1.50"),
    pairWireless: vi.fn(async () => {}),
    setRecordDir: vi.fn(async (path: string | null) => ({
      effective: path ?? "/Users/me/Desktop",
      accepted: true,
      message: null,
    })),
    openKeyboardSettings: vi.fn(async () => {}),
    ...overrides,
  };
}

// All Launcher tests use null record-dir + no-op callbacks; this helper keeps
// the assertion lines short and one place to update if props grow again.
function renderLauncher(
  backend: Backend,
  overrides: Partial<{
    lastPresetId: string | null;
    recordDir: string | null;
    ipHistory: string[];
    onPresetUsed: (id: string) => void;
    onRecordDirChanged: (d: string | null) => void;
    onIpUsed: (ip: string) => void;
  }> = {},
) {
  return render(
    <Launcher
      backend={backend}
      lastPresetId={overrides.lastPresetId ?? null}
      recordDir={overrides.recordDir ?? null}
      ipHistory={overrides.ipHistory ?? []}
      onPresetUsed={overrides.onPresetUsed ?? (() => {})}
      onRecordDirChanged={overrides.onRecordDirChanged ?? (() => {})}
      onIpUsed={overrides.onIpUsed ?? (() => {})}
    />,
  );
}

const pixel: Device = { serial: "R5CX21RJ6MX", model: "Pixel 7", rawState: "device" };

beforeEach(() => vi.useRealTimers());
afterEach(() => cleanup());

describe("Launcher device states", () => {
  it("shows adb-missing state when adb is unavailable", async () => {
    const backend = fakeBackend({ adbAvailable: vi.fn(async () => false) });
    renderLauncher(backend);
    expect(await screen.findByText("安装 adb")).toBeInTheDocument();
  });

  it("shows empty state when no devices", async () => {
    const backend = fakeBackend();
    renderLauncher(backend);
    expect(await screen.findByText("未检测到设备")).toBeInTheDocument();
  });

  it("shows unauthorized state", async () => {
    const backend = fakeBackend({
      listDevices: vi.fn(async () => [{ serial: "ABC12345", model: null, rawState: "unauthorized" }]),
    });
    renderLauncher(backend);
    expect(await screen.findByText("设备已连接，等待授权")).toBeInTheDocument();
  });

  it("shows connected device model + serial", async () => {
    const backend = fakeBackend({ listDevices: vi.fn(async () => [pixel]) });
    renderLauncher(backend);
    expect(await screen.findByText("Pixel 7")).toBeInTheDocument();
    expect(screen.getByText("R5CX21RJ6MX")).toBeInTheDocument();
  });

  it("shows device count for multiple devices", async () => {
    const backend = fakeBackend({
      listDevices: vi.fn(async () => [pixel, { serial: "BBB22222", model: "Galaxy", rawState: "device" }]),
    });
    renderLauncher(backend);
    expect(await screen.findByText("共 2 台")).toBeInTheDocument();
  });
});

describe("Launcher preset behavior", () => {
  it("puts last-used preset in the primary slot (PRD D2)", async () => {
    const backend = fakeBackend({ listDevices: vi.fn(async () => [pixel]) });
    renderLauncher(backend, { lastPresetId: "game-low-latency" });
    const primary = await screen.findByRole("button", { name: /游戏低延迟/ });
    expect(primary.className).toContain("primary-launch");
  });

  it("falls back to first preset on first run (no lastPresetId)", async () => {
    const backend = fakeBackend({ listDevices: vi.fn(async () => [pixel]) });
    renderLauncher(backend);
    const primary = await screen.findByText("高画质启动");
    expect(primary.closest("button")?.className).toContain("primary-launch");
  });

  it("launches and reports the used preset", async () => {
    const user = userEvent.setup();
    const launchSpy = vi.fn(async (_serial: string, _preset: Preset) => {});
    const usedSpy = vi.fn();
    const backend = fakeBackend({ listDevices: vi.fn(async () => [pixel]), launchScrcpy: launchSpy });
    renderLauncher(backend, { onPresetUsed: usedSpy });
    const primary = await screen.findByText("高画质启动");
    await user.click(primary.closest("button")!);
    await waitFor(() => expect(launchSpy).toHaveBeenCalledOnce());
    const [serial, preset] = launchSpy.mock.calls[0];
    expect(serial).toBe("R5CX21RJ6MX");
    expect(preset.id).toBe("high-quality");
    expect(usedSpy).toHaveBeenCalledWith("high-quality");
  });

  it("surfaces an error bar when launch fails", async () => {
    const user = userEvent.setup();
    const backend = fakeBackend({
      listDevices: vi.fn(async () => [pixel]),
      launchScrcpy: vi.fn(async () => {
        throw { kind: "ScrcpyLaunchFailed", message: "binary not found" };
      }),
    });
    renderLauncher(backend);
    const primary = await screen.findByText("高画质启动");
    await user.click(primary.closest("button")!);
    expect(await screen.findByRole("alert")).toHaveTextContent("binary not found");
  });

  it("disables launch when no device is connected", async () => {
    const backend = fakeBackend();
    renderLauncher(backend);
    await screen.findByText("未检测到设备");
    const primary = screen.getByText("高画质启动").closest("button")!;
    expect(primary).toBeDisabled();
  });
});

describe("Launcher record-dir setting", () => {
  it("shows the default placeholder when no recordDir is set", () => {
    renderLauncher(fakeBackend());
    expect(screen.getByText(/默认.*Desktop/)).toBeInTheDocument();
  });

  it("shows the persisted recordDir when one is set", () => {
    renderLauncher(fakeBackend(), { recordDir: "/Users/me/Movies/scrcpy" });
    expect(screen.getByText("/Users/me/Movies/scrcpy")).toBeInTheDocument();
  });

  it("surfaces the warning and KEEPS the previous value when the backend rejects the chosen dir", async () => {
    // setRecordDir is called by the chooseRecordDir handler; we make it reject
    // the user's pick and report the prior effective path.
    const setRecordDir = vi.fn(async (_p: string | null) => ({
      effective: "/Users/me/Desktop",
      accepted: false,
      message: "目录不可写: Permission denied",
    }));
    const onChanged = vi.fn();
    const backend = fakeBackend({ setRecordDir });

    // Stub the dialog dynamic import so the test can simulate a user pick
    // without bringing up a real macOS folder picker.
    vi.doMock("@tauri-apps/plugin-dialog", () => ({
      open: vi.fn(async () => "/no/perm"),
    }));

    const user = userEvent.setup();
    renderLauncher(backend, { onRecordDirChanged: onChanged });
    await user.click(screen.getByRole("button", { name: "更改..." }));
    expect(await screen.findByText(/目录不可写/)).toBeInTheDocument();
    expect(setRecordDir).toHaveBeenCalledWith("/no/perm");
    // Rejected → don't persist (would otherwise re-replay a bad path).
    expect(onChanged).not.toHaveBeenCalled();
    vi.doUnmock("@tauri-apps/plugin-dialog");
  });
});

describe("Launcher wireless connection", () => {
  const pixelOnline = { serial: "R5CX21RJ6MX", model: "Pixel 7", rawState: "device" };

  it("is collapsed by default and expands on click", async () => {
    const user = userEvent.setup();
    renderLauncher(fakeBackend());
    // Body hidden until toggled.
    expect(screen.queryByText(/方式 A/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /无线连接/ }));
    expect(screen.getByText(/方式 A/)).toBeInTheDocument();
    expect(screen.getByText(/方式 B/)).toBeInTheDocument();
  });

  it("enableTcpip pre-fills the connect field with the returned IP", async () => {
    const user = userEvent.setup();
    const enableTcpip = vi.fn(async () => "192.168.1.50");
    const backend = fakeBackend({
      listDevices: vi.fn(async () => [pixelOnline]),
      enableTcpip,
    });
    renderLauncher(backend);
    await screen.findByText("Pixel 7"); // wait for device poll
    await user.click(screen.getByRole("button", { name: /无线连接/ }));
    await user.click(screen.getByRole("button", { name: "切换到无线模式" }));
    await waitFor(() => expect(enableTcpip).toHaveBeenCalledWith("R5CX21RJ6MX"));
    expect(await screen.findByDisplayValue("192.168.1.50:5555")).toBeInTheDocument();
  });

  it("connect persists the address via onIpUsed", async () => {
    const user = userEvent.setup();
    const connectWireless = vi.fn(async () => {});
    const onIpUsed = vi.fn();
    const backend = fakeBackend({ connectWireless });
    renderLauncher(backend, { onIpUsed });
    await user.click(screen.getByRole("button", { name: /无线连接/ }));
    const input = screen.getByPlaceholderText("192.168.x.x:5555");
    await user.type(input, "10.0.0.7:5555");
    await user.click(screen.getByRole("button", { name: "连接" }));
    await waitFor(() => expect(connectWireless).toHaveBeenCalledWith("10.0.0.7:5555"));
    expect(onIpUsed).toHaveBeenCalledWith("10.0.0.7:5555");
  });

  it("renders history chips and re-connects when one is clicked", async () => {
    const user = userEvent.setup();
    const connectWireless = vi.fn(async () => {});
    const backend = fakeBackend({ connectWireless });
    renderLauncher(backend, { ipHistory: ["192.168.1.9:5555"] });
    await user.click(screen.getByRole("button", { name: /无线连接/ }));
    await user.click(screen.getByRole("button", { name: "192.168.1.9:5555" }));
    await waitFor(() => expect(connectWireless).toHaveBeenCalledWith("192.168.1.9:5555"));
  });

  it("pairing calls pairWireless with ip/port/code", async () => {
    const user = userEvent.setup();
    const pairWireless = vi.fn(async () => {});
    const backend = fakeBackend({ pairWireless });
    renderLauncher(backend);
    await user.click(screen.getByRole("button", { name: /无线连接/ }));
    await user.type(screen.getByPlaceholderText("配对 IP"), "192.168.1.9");
    await user.type(screen.getByPlaceholderText("端口"), "37123");
    await user.type(screen.getByPlaceholderText("6 位配对码"), "123456");
    await user.click(screen.getByRole("button", { name: "配对" }));
    await waitFor(() =>
      expect(pairWireless).toHaveBeenCalledWith("192.168.1.9", "37123", "123456"),
    );
  });

  it("shows the backend error message when connect fails", async () => {
    const user = userEvent.setup();
    const connectWireless = vi.fn(async () => {
      throw { kind: "WirelessConnectFailed", message: "connection refused" };
    });
    const backend = fakeBackend({ connectWireless });
    renderLauncher(backend);
    await user.click(screen.getByRole("button", { name: /无线连接/ }));
    await user.type(screen.getByPlaceholderText("192.168.x.x:5555"), "10.0.0.7:5555");
    await user.click(screen.getByRole("button", { name: "连接" }));
    expect(await screen.findByText(/connection refused/)).toBeInTheDocument();
  });
});
