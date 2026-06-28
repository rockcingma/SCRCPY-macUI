import { describe, it, expect } from "vitest";
import {
  pushIpHistory,
  isValidIp,
  isValidSerial,
  coerceSettings,
  DEFAULT_SETTINGS,
  MAX_IP_HISTORY,
} from "../store/settings";

describe("pushIpHistory", () => {
  it("appends a new IP at the front", () => {
    expect(pushIpHistory([], "192.168.1.1")).toEqual(["192.168.1.1"]);
  });

  it("dedups, moving an existing IP to the front", () => {
    expect(pushIpHistory(["a", "b", "c"], "c")).toEqual(["c", "a", "b"]);
  });

  it("caps history at MAX_IP_HISTORY", () => {
    const start = ["1", "2", "3", "4", "5"];
    const result = pushIpHistory(start, "6");
    expect(result).toHaveLength(MAX_IP_HISTORY);
    expect(result[0]).toBe("6");
    expect(result).not.toContain("5");
  });

  it("ignores empty/whitespace input", () => {
    expect(pushIpHistory(["a"], "   ")).toEqual(["a"]);
    expect(pushIpHistory(["a"], "")).toEqual(["a"]);
  });

  it("trims whitespace before storing", () => {
    expect(pushIpHistory([], "  192.168.0.1  ")).toEqual(["192.168.0.1"]);
  });
});

describe("isValidIp", () => {
  it("accepts plain IPv4", () => {
    expect(isValidIp("192.168.1.100")).toBe(true);
  });
  it("accepts IPv4 with port", () => {
    expect(isValidIp("192.168.1.100:5555")).toBe(true);
  });
  it("rejects octets > 255", () => {
    expect(isValidIp("999.1.1.1")).toBe(false);
  });
  it("rejects port out of range", () => {
    expect(isValidIp("192.168.1.1:70000")).toBe(false);
  });
  it("rejects garbage", () => {
    expect(isValidIp("not-an-ip")).toBe(false);
    expect(isValidIp("192.168.1")).toBe(false);
  });
});

describe("isValidSerial", () => {
  it("accepts typical adb serials", () => {
    expect(isValidSerial("R5CX21RJ6MX")).toBe(true);
  });
  it("rejects too-short serials", () => {
    expect(isValidSerial("ABC")).toBe(false);
  });
  it("rejects shell metacharacters (injection guard)", () => {
    expect(isValidSerial("abc; rm -rf /")).toBe(false);
    expect(isValidSerial("$(whoami)")).toBe(false);
  });
});

describe("coerceSettings", () => {
  it("returns defaults for null/garbage", () => {
    expect(coerceSettings(null)).toEqual(DEFAULT_SETTINGS);
    expect(coerceSettings("nope")).toEqual(DEFAULT_SETTINGS);
    expect(coerceSettings(42)).toEqual(DEFAULT_SETTINGS);
  });
  it("preserves valid fields", () => {
    expect(coerceSettings({ lastPresetId: "x", ipHistory: ["1.2.3.4"] })).toEqual({
      lastPresetId: "x",
      ipHistory: ["1.2.3.4"],
    });
  });
  it("drops non-string ipHistory entries and caps length", () => {
    const raw = { lastPresetId: 5, ipHistory: ["a", 2, "b", null, "c", "d", "e", "f"] };
    const result = coerceSettings(raw);
    expect(result.lastPresetId).toBeNull();
    expect(result.ipHistory).toEqual(["a", "b", "c", "d", "e"]);
  });
});
