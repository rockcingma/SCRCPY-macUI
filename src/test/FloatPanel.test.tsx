import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FloatPanel } from "../FloatPanel";
import type { Backend } from "../backend";
import { FLOAT_BUTTONS, type KeyAction } from "../types";

function fakeBackend(overrides: Partial<Backend> = {}): Backend {
  return {
    adbAvailable: vi.fn(async () => true),
    listDevices: vi.fn(async () => []),
    launchScrcpy: vi.fn(async () => {}),
    connectWireless: vi.fn(async () => {}),
    sendKey: vi.fn(async () => {}),
    toggleRecording: vi.fn(async () => ({ recording: false, savedPath: null })),
    toggleScreenOff: vi.fn(async () => ({ screenOff: false })),
    toggleAudioHost: vi.fn(async () => ({ hostAudio: true })),
    toggleAlwaysOnTop: vi.fn(async () => ({ alwaysOnTop: false })),
    enableTcpip: vi.fn(async () => "192.168.1.50"),
    pairWireless: vi.fn(async () => {}),
    setRecordDir: vi.fn(async () => ({
      effective: "/Users/me/Desktop",
      accepted: true,
      message: null,
    })),
    openKeyboardSettings: vi.fn(async () => {}),
    ...overrides,
  };
}

beforeEach(() => vi.useRealTimers());
afterEach(() => cleanup());

describe("FloatPanel rendering", () => {
  it("renders all 10 buttons defined in FLOAT_BUTTONS", () => {
    render(<FloatPanel backend={fakeBackend()} />);
    for (const b of FLOAT_BUTTONS) {
      expect(screen.getByRole("button", { name: b.label })).toBeInTheDocument();
    }
  });

  it("renders the drag handle", () => {
    render(<FloatPanel backend={fakeBackend()} />);
    expect(screen.getByTitle("拖动")).toBeInTheDocument();
  });

  it("does not render any accessibility-permission UI (adb needs none)", () => {
    render(<FloatPanel backend={fakeBackend()} />);
    expect(screen.queryByText("需开启辅助功能")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "打开设置" })).not.toBeInTheDocument();
  });
});

describe("FloatPanel interactions", () => {
  it("dispatches the correct KeyAction when a button is clicked", async () => {
    const user = userEvent.setup();
    const sendKey = vi.fn(async (_action: KeyAction) => {});
    render(<FloatPanel backend={fakeBackend({ sendKey })} />);
    await user.click(screen.getByRole("button", { name: "主屏幕" }));
    await waitFor(() => expect(sendKey).toHaveBeenCalledWith("home"));
  });

  it("dispatches every action when its button is clicked", async () => {
    const user = userEvent.setup();
    const sendKey = vi.fn(async (_action: KeyAction) => {});
    render(<FloatPanel backend={fakeBackend({ sendKey })} />);
    for (const b of FLOAT_BUTTONS) {
      await user.click(screen.getByRole("button", { name: b.label }));
    }
    const calls = sendKey.mock.calls.map((c) => c[0]);
    expect(calls).toEqual(FLOAT_BUTTONS.map((b) => b.action));
  });

  it("applies press-flash class for ~80ms after a click", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(<FloatPanel backend={fakeBackend()} />);
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
        throw { kind: "KeyInjectFailed", message: "device offline" };
      }),
    });
    render(<FloatPanel backend={backend} />);
    const btn = screen.getByRole("button", { name: "主屏幕" });
    await user.click(btn);
    await waitFor(() => expect(btn.className).toContain("flash-error"));
    vi.advanceTimersByTime(200);
    await waitFor(() => expect(btn.className).not.toContain("flash-error"));
  });

  it("recovers and stays interactive after a failed press", async () => {
    const user = userEvent.setup();
    const sendKey = vi
      .fn()
      .mockRejectedValueOnce({ kind: "KeyInjectFailed", message: "x" })
      .mockResolvedValueOnce(undefined);
    render(<FloatPanel backend={fakeBackend({ sendKey })} />);
    const btn = screen.getByRole("button", { name: "返回" });
    await user.click(btn); // fails
    await user.click(btn); // succeeds
    await waitFor(() => expect(sendKey).toHaveBeenCalledTimes(2));
    expect(sendKey.mock.calls.every((c) => c[0] === "back")).toBe(true);
  });
});

describe("FloatPanel recording toggle", () => {
  it("renders a record button in the idle (start) state", () => {
    render(<FloatPanel backend={fakeBackend()} />);
    expect(screen.getByRole("button", { name: "开始录制" })).toBeInTheDocument();
  });

  it("calls toggleRecording and switches to recording state on start", async () => {
    const user = userEvent.setup();
    const toggleRecording = vi.fn(async () => ({ recording: true, savedPath: null }));
    render(<FloatPanel backend={fakeBackend({ toggleRecording })} />);
    await user.click(screen.getByRole("button", { name: "开始录制" }));
    // Button flips to the stop affordance.
    expect(await screen.findByRole("button", { name: "停止录制" })).toBeInTheDocument();
    expect(toggleRecording).toHaveBeenCalledOnce();
  });

  it("shows a saved toast with the filename after stopping", async () => {
    const user = userEvent.setup();
    // First click starts, second stops and returns a saved path.
    const toggleRecording = vi
      .fn()
      .mockResolvedValueOnce({ recording: true, savedPath: null })
      .mockResolvedValueOnce({
        recording: false,
        savedPath: "/Users/me/Desktop/scrcpy-DEV-123.mp4",
      });
    render(<FloatPanel backend={fakeBackend({ toggleRecording })} />);
    await user.click(screen.getByRole("button", { name: "开始录制" }));
    await user.click(await screen.findByRole("button", { name: "停止录制" }));
    // Toast shows just the filename, not the full path.
    expect(await screen.findByText(/已保存 scrcpy-DEV-123\.mp4/)).toBeInTheDocument();
    // And the button is back to the start state.
    expect(screen.getByRole("button", { name: "开始录制" })).toBeInTheDocument();
  });

  it("ignores double-clicks while a toggle is in flight", async () => {
    const user = userEvent.setup();
    // Never-resolving toggle keeps the button in the busy/disabled state.
    let resolve: (v: { recording: boolean; savedPath: string | null }) => void = () => {};
    const toggleRecording = vi.fn(
      () => new Promise<{ recording: boolean; savedPath: string | null }>((r) => { resolve = r; }),
    );
    render(<FloatPanel backend={fakeBackend({ toggleRecording })} />);
    const btn = screen.getByRole("button", { name: "开始录制" });
    await user.click(btn);
    await user.click(btn); // should be ignored (disabled + busy guard)
    expect(toggleRecording).toHaveBeenCalledOnce();
    resolve({ recording: true, savedPath: null }); // cleanup
  });

  it("stays in its prior state if the toggle backend call fails", async () => {
    const user = userEvent.setup();
    const toggleRecording = vi.fn(async () => {
      throw { kind: "ScrcpyLaunchFailed", message: "boom" };
    });
    render(<FloatPanel backend={fakeBackend({ toggleRecording })} />);
    await user.click(screen.getByRole("button", { name: "开始录制" }));
    // Failure leaves it in the start state, re-enabled for another try.
    expect(await screen.findByRole("button", { name: "开始录制" })).toBeEnabled();
  });
});

describe("FloatPanel screen-off toggle", () => {
  it("calls toggleScreenOff and swaps the button label after the click", async () => {
    const user = userEvent.setup();
    const toggleScreenOff = vi.fn(async () => ({ screenOff: true }));
    render(<FloatPanel backend={fakeBackend({ toggleScreenOff })} />);
    await user.click(screen.getByRole("button", { name: "关闭手机屏幕" }));
    expect(toggleScreenOff).toHaveBeenCalledOnce();
    // After the backend reports screenOff=true, the label flips to the
    // inverse action so the same button toggles back.
    expect(await screen.findByRole("button", { name: "开启手机屏幕" })).toBeInTheDocument();
  });

  it("triggers toggleScreenOff when Cmd+O is pressed", async () => {
    const user = userEvent.setup();
    const toggleScreenOff = vi.fn(async () => ({ screenOff: true }));
    render(<FloatPanel backend={fakeBackend({ toggleScreenOff })} />);
    await user.keyboard("{Meta>}o{/Meta}");
    await waitFor(() => expect(toggleScreenOff).toHaveBeenCalledOnce());
  });

  it("does not fire on Cmd+Shift+O (only bare Cmd+O is the shortcut)", async () => {
    // Guards against accidentally swallowing Cmd+Shift+O or Cmd+Alt+O, which
    // other parts of the app might want for their own bindings.
    const user = userEvent.setup();
    const toggleScreenOff = vi.fn(async () => ({ screenOff: true }));
    render(<FloatPanel backend={fakeBackend({ toggleScreenOff })} />);
    await user.keyboard("{Meta>}{Shift>}o{/Shift}{/Meta}");
    expect(toggleScreenOff).not.toHaveBeenCalled();
  });
});

describe("FloatPanel audio-host toggle", () => {
  it("starts in Mac-plays mode and switches to device on click", async () => {
    const user = userEvent.setup();
    const toggleAudioHost = vi.fn(async () => ({ hostAudio: false }));
    render(<FloatPanel backend={fakeBackend({ toggleAudioHost })} />);
    await user.click(screen.getByRole("button", { name: "切换到手机播放" }));
    expect(toggleAudioHost).toHaveBeenCalledOnce();
    expect(await screen.findByRole("button", { name: "切换到 Mac 播放" })).toBeInTheDocument();
  });
});

describe("FloatPanel button order", () => {
  it("renders the record button just before the close button (user-requested order)", () => {
    render(<FloatPanel backend={fakeBackend()} />);
    // Find every panel button (in DOM order) and pull out the labels we care
    // about — the last two should be 开始录制, 关闭投屏 in that order.
    const labels = Array.from(
      document.querySelectorAll<HTMLButtonElement>(".float-panel button"),
    ).map((el) => el.getAttribute("aria-label"));
    const recordIdx = labels.indexOf("开始录制");
    const closeIdx = labels.indexOf("关闭投屏");
    expect(recordIdx).toBeGreaterThan(-1);
    expect(closeIdx).toBeGreaterThan(-1);
    expect(closeIdx).toBe(recordIdx + 1);
    // And close really is the very last button in the panel.
    expect(closeIdx).toBe(labels.length - 1);
  });
});
