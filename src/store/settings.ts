// Pure settings logic — no Tauri imports, fully unit-testable.
// The persistence layer (store.ts) wraps these with tauri-plugin-store.

export const MAX_IP_HISTORY = 5;

// Append an IP to history: dedup (most-recent-first) and cap at MAX_IP_HISTORY.
// Returns a new array; never mutates the input.
export function pushIpHistory(history: string[], ip: string): string[] {
  const trimmed = ip.trim();
  if (trimmed === "") return history;
  const deduped = history.filter((h) => h !== trimmed);
  return [trimmed, ...deduped].slice(0, MAX_IP_HISTORY);
}

// Validate an IPv4 address with optional :port (PRD §5.2 whitelist).
export function isValidIp(input: string): boolean {
  const m = input.trim().match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})(:(\d{1,5}))?$/);
  if (!m) return false;
  const octets = [m[1], m[2], m[3], m[4]].map(Number);
  if (octets.some((o) => o > 255)) return false;
  if (m[6] !== undefined) {
    const port = Number(m[6]);
    if (port < 1 || port > 65535) return false;
  }
  return true;
}

// Validate an adb device serial (PRD §5.2 whitelist).
export function isValidSerial(serial: string): boolean {
  return /^[A-Za-z0-9]{8,32}$/.test(serial);
}

export interface Settings {
  lastPresetId: string | null;
  ipHistory: string[];
}

export const DEFAULT_SETTINGS: Settings = {
  lastPresetId: null,
  ipHistory: [],
};

// Coerce arbitrary persisted JSON into a valid Settings object.
// Guards against corrupt/partial store files (PRD test: empty file → defaults).
export function coerceSettings(raw: unknown): Settings {
  if (raw === null || typeof raw !== "object") return { ...DEFAULT_SETTINGS };
  const obj = raw as Record<string, unknown>;
  const lastPresetId = typeof obj.lastPresetId === "string" ? obj.lastPresetId : null;
  const ipHistory = Array.isArray(obj.ipHistory)
    ? obj.ipHistory.filter((x): x is string => typeof x === "string").slice(0, MAX_IP_HISTORY)
    : [];
  return { lastPresetId, ipHistory };
}
