import { describe, it, expect } from "vitest";
import { deriveStatus, stateLabel, stateDot } from "../device";
import type { Device } from "../types";

const dev = (serial: string, rawState: string, model: string | null = null): Device => ({
  serial,
  rawState,
  model,
});

describe("deriveStatus", () => {
  it("empty list → empty", () => {
    expect(deriveStatus([]).state).toBe("empty");
  });

  it("single ready device → connected, active set", () => {
    const r = deriveStatus([dev("R5CX21RJ6MX", "device", "Pixel 7")]);
    expect(r.state).toBe("connected");
    expect(r.active?.serial).toBe("R5CX21RJ6MX");
  });

  it("only unauthorized device → unauthorized, no active", () => {
    const r = deriveStatus([dev("ABC12345", "unauthorized")]);
    expect(r.state).toBe("unauthorized");
    expect(r.active).toBeNull();
  });

  it("offline-only device → empty (not actionable)", () => {
    const r = deriveStatus([dev("ABC12345", "offline")]);
    expect(r.state).toBe("empty");
    expect(r.active).toBeNull();
  });

  it("two ready devices → multiple, active is first ready", () => {
    const r = deriveStatus([dev("AAA11111", "device"), dev("BBB22222", "device")]);
    expect(r.state).toBe("multiple");
    expect(r.active?.serial).toBe("AAA11111");
    expect(r.devices).toHaveLength(2);
  });

  it("mix of ready + unauthorized → connected (ready wins)", () => {
    const r = deriveStatus([dev("AAA11111", "device"), dev("BBB22222", "unauthorized")]);
    expect(r.state).toBe("connected");
    expect(r.active?.serial).toBe("AAA11111");
  });
});

describe("stateLabel / stateDot", () => {
  it("covers all six states with a label and a dot color", () => {
    const states = [
      "detecting",
      "empty",
      "unauthorized",
      "adb_missing",
      "connected",
      "multiple",
    ] as const;
    for (const s of states) {
      expect(stateLabel(s).length).toBeGreaterThan(0);
      expect(["gray", "yellow", "orange", "green", "blue"]).toContain(stateDot(s));
    }
  });
});
