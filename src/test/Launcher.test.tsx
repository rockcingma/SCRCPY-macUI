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
    ...overrides,
  };
}

const pixel: Device = { serial: "R5CX21RJ6MX", model: "Pixel 7", rawState: "device" };

beforeEach(() => vi.useRealTimers());
afterEach(() => cleanup());

describe("Launcher device states", () => {
  it("shows adb-missing state when adb is unavailable", async () => {
    const backend = fakeBackend({ adbAvailable: vi.fn(async () => false) });
    render(<Launcher backend={backend} lastPresetId={null} onPresetUsed={() => {}} />);
    expect(await screen.findByText("安装 adb")).toBeInTheDocument();
  });

  it("shows empty state when no devices", async () => {
    const backend = fakeBackend();
    render(<Launcher backend={backend} lastPresetId={null} onPresetUsed={() => {}} />);
    expect(await screen.findByText("未检测到设备")).toBeInTheDocument();
  });

  it("shows unauthorized state", async () => {
    const backend = fakeBackend({
      listDevices: vi.fn(async () => [{ serial: "ABC12345", model: null, rawState: "unauthorized" }]),
    });
    render(<Launcher backend={backend} lastPresetId={null} onPresetUsed={() => {}} />);
    expect(await screen.findByText("设备已连接，等待授权")).toBeInTheDocument();
  });

  it("shows connected device model + serial", async () => {
    const backend = fakeBackend({ listDevices: vi.fn(async () => [pixel]) });
    render(<Launcher backend={backend} lastPresetId={null} onPresetUsed={() => {}} />);
    expect(await screen.findByText("Pixel 7")).toBeInTheDocument();
    expect(screen.getByText("R5CX21RJ6MX")).toBeInTheDocument();
  });

  it("shows device count for multiple devices", async () => {
    const backend = fakeBackend({
      listDevices: vi.fn(async () => [pixel, { serial: "BBB22222", model: "Galaxy", rawState: "device" }]),
    });
    render(<Launcher backend={backend} lastPresetId={null} onPresetUsed={() => {}} />);
    expect(await screen.findByText("共 2 台")).toBeInTheDocument();
  });
});

describe("Launcher preset behavior", () => {
  it("puts last-used preset in the primary slot (PRD D2)", async () => {
    const backend = fakeBackend({ listDevices: vi.fn(async () => [pixel]) });
    render(<Launcher backend={backend} lastPresetId="game-low-latency" onPresetUsed={() => {}} />);
    const primary = await screen.findByRole("button", { name: /游戏低延迟/ });
    expect(primary.className).toContain("primary-launch");
  });

  it("falls back to first preset on first run (no lastPresetId)", async () => {
    const backend = fakeBackend({ listDevices: vi.fn(async () => [pixel]) });
    render(<Launcher backend={backend} lastPresetId={null} onPresetUsed={() => {}} />);
    const primary = await screen.findByText("高画质启动");
    expect(primary.closest("button")?.className).toContain("primary-launch");
  });

  it("launches and reports the used preset", async () => {
    const user = userEvent.setup();
    const launchSpy = vi.fn(async () => {});
    const usedSpy = vi.fn();
    const backend = fakeBackend({ listDevices: vi.fn(async () => [pixel]), launchScrcpy: launchSpy });
    render(<Launcher backend={backend} lastPresetId={null} onPresetUsed={usedSpy} />);
    const primary = await screen.findByText("高画质启动");
    await user.click(primary.closest("button")!);
    await waitFor(() => expect(launchSpy).toHaveBeenCalledOnce());
    const [serial, preset] = launchSpy.mock.calls[0] as [string, Preset];
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
    render(<Launcher backend={backend} lastPresetId={null} onPresetUsed={() => {}} />);
    const primary = await screen.findByText("高画质启动");
    await user.click(primary.closest("button")!);
    expect(await screen.findByRole("alert")).toHaveTextContent("binary not found");
  });

  it("disables launch when no device is connected", async () => {
    const backend = fakeBackend();
    render(<Launcher backend={backend} lastPresetId={null} onPresetUsed={() => {}} />);
    await screen.findByText("未检测到设备");
    const primary = screen.getByText("高画质启动").closest("button")!;
    expect(primary).toBeDisabled();
  });
});
