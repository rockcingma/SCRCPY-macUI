import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FloatPanel } from "../FloatPanel";
import type { Backend } from "../backend";
import { FLOAT_BUTTONS } from "../types";

function fakeBackend(overrides: Partial<Backend> = {}): Backend {
  return {
    adbAvailable: vi.fn(async () => true),
    listDevices: vi.fn(async () => []),
    launchScrcpy: vi.fn(async () => {}),
    connectWireless: vi.fn(async () => {}),
    sendKey: vi.fn(async () => {}),
    accessibilityStatus: vi.fn(async () => true),
    openAccessibilitySettings: vi.fn(async () => {}),
    ...overrides,
  };
}

beforeEach(() => vi.useRealTimers());
afterEach(() => cleanup());

describe("FloatPanel rendering", () => {
  it("renders all 10 buttons defined in FLOAT_BUTTONS", () => {
    const backend = fakeBackend();
    render(<FloatPanel backend={backend} />);
    for (const b of FLOAT_BUTTONS) {
      expect(screen.getByRole("button", { name: b.label })).toBeInTheDocument();
    }
  });

  it("does not show accessibility warning when osascript works", async () => {
    const backend = fakeBackend({ accessibilityStatus: vi.fn(async () => true) });
    render(<FloatPanel backend={backend} />);
    await waitFor(() => expect(backend.accessibilityStatus).toHaveBeenCalled());
    expect(screen.queryByText("需开启辅助功能")).not.toBeInTheDocument();
  });

  it("shows accessibility warning when osascript is denied", async () => {
    const backend = fakeBackend({ accessibilityStatus: vi.fn(async () => false) });
    render(<FloatPanel backend={backend} />);
    expect(await screen.findByText("需开启辅助功能")).toBeInTheDocument();
  });
});

describe("FloatPanel interactions", () => {
  it("dispatches the correct KeyAction when a button is clicked", async () => {
    const user = userEvent.setup();
    const sendKey = vi.fn(async () => {});
    const backend = fakeBackend({ sendKey });
    render(<FloatPanel backend={backend} />);
    await user.click(screen.getByRole("button", { name: "主屏幕" }));
    await waitFor(() => expect(sendKey).toHaveBeenCalledWith("home"));
  });

  it("dispatches every action when its button is clicked", async () => {
    const user = userEvent.setup();
    const sendKey = vi.fn(async () => {});
    const backend = fakeBackend({ sendKey });
    render(<FloatPanel backend={backend} />);
    for (const b of FLOAT_BUTTONS) {
      await user.click(screen.getByRole("button", { name: b.label }));
    }
    const calls = sendKey.mock.calls.map((c) => c[0]);
    expect(calls).toEqual(FLOAT_BUTTONS.map((b) => b.action));
  });

  it("applies press-flash class for ~80ms after a click", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    const backend = fakeBackend();
    render(<FloatPanel backend={backend} />);
    const btn = screen.getByRole("button", { name: "主屏幕" });
    await user.click(btn);
    expect(btn.className).toContain("flash-press");
    vi.advanceTimersByTime(100);
    await waitFor(() => expect(btn.className).not.toContain("flash-press"));
  });

  it("applies error-flash when the backend rejects", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    const backend = fakeBackend({
      sendKey: vi.fn(async () => {
        throw { kind: "KeyInjectFailed", message: "no scrcpy window" };
      }),
    });
    render(<FloatPanel backend={backend} />);
    const btn = screen.getByRole("button", { name: "主屏幕" });
    await user.click(btn);
    await waitFor(() => expect(btn.className).toContain("flash-error"));
    vi.advanceTimersByTime(200);
    await waitFor(() => expect(btn.className).not.toContain("flash-error"));
  });

  it("surfaces accessibility banner when sendKey reports AccessibilityDenied", async () => {
    const user = userEvent.setup();
    const backend = fakeBackend({
      // Pretend the initial probe passed, but a real keystroke is denied.
      accessibilityStatus: vi.fn(async () => true),
      sendKey: vi.fn(async () => {
        throw { kind: "AccessibilityDenied" };
      }),
    });
    render(<FloatPanel backend={backend} />);
    await user.click(screen.getByRole("button", { name: "主屏幕" }));
    expect(await screen.findByText("需开启辅助功能")).toBeInTheDocument();
  });

  it("invokes the backend to open System Settings (not webview navigation)", async () => {
    const user = userEvent.setup();
    const openSettings = vi.fn(async () => {});
    const backend = fakeBackend({
      accessibilityStatus: vi.fn(async () => false),
      openAccessibilitySettings: openSettings,
    });
    render(<FloatPanel backend={backend} />);
    const link = await screen.findByRole("button", { name: "打开设置" });
    await user.click(link);
    expect(openSettings).toHaveBeenCalledOnce();
  });

  it("re-probes accessibility when the user clicks 重试 after granting", async () => {
    const user = userEvent.setup();
    // Sequence: first probe denies, second probe passes.
    const probe = vi.fn()
      .mockResolvedValueOnce(false)
      .mockResolvedValueOnce(true);
    const backend = fakeBackend({ accessibilityStatus: probe });
    render(<FloatPanel backend={backend} />);
    expect(await screen.findByText("需开启辅助功能")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "已授权,重试" }));
    await waitFor(() =>
      expect(screen.queryByText("需开启辅助功能")).not.toBeInTheDocument(),
    );
    expect(probe).toHaveBeenCalledTimes(2);
  });
});
