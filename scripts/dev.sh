#!/bin/bash
# Dev launcher with stable Accessibility grants (macOS 26 Tahoe workaround).
#
# tauri-cli's `dev` command always runs `cargo run --no-default-features`
# under the hood, which relinks the binary with cargo's default ad-hoc
# identifier (a per-build hash). macOS 26 TCC keys Accessibility grants on
# the codesign identifier, so every relink wipes the grant.
#
# Workaround: drive cargo + the binary ourselves.
#   1. cargo build → binary on disk
#   2. codesign with a stable identifier (matches the TCC entry)
#   3. spawn vite separately for frontend HMR
#   4. exec the signed binary directly — no `cargo run`, no relink
#
# Tradeoff: Rust code changes don't auto-restart the app. Re-run this
# script after editing src-tauri/. The first ./scripts/dev.sh that
# succeeds prompts macOS once; subsequent runs reuse the grant.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BIN="src-tauri/target/debug/scrcpy-mac-ui"
IDENTIFIER="com.scrcpy.controller.dev"
VITE_PORT=1420

source "$HOME/.cargo/env" 2>/dev/null || true

cleanup() {
  echo ""
  echo "▸ stopping dev session..."
  [ -n "${VITE_PID:-}" ] && kill "$VITE_PID" 2>/dev/null || true
  [ -n "${APP_PID:-}" ] && kill "$APP_PID" 2>/dev/null || true
  exit 0
}
trap cleanup INT TERM

# 1. Build the binary.
echo "▸ cargo build..."
cargo build \
  --manifest-path src-tauri/Cargo.toml \
  --no-default-features

# 2. Stable ad-hoc codesign.
echo "▸ codesign (stable identity: $IDENTIFIER)..."
codesign --force --sign - --identifier "$IDENTIFIER" "$BIN"
SIGNED_ID=$(codesign -dv "$BIN" 2>&1 | awk -F= '/^Identifier=/{print $2}')
[ "$SIGNED_ID" = "$IDENTIFIER" ] || { echo "❌ codesign id mismatch: $SIGNED_ID"; exit 1; }
echo "  signature: $SIGNED_ID"

# 3. Vite for the frontend (Tauri WebView points at http://localhost:1420).
#    Use bun run so the script command resolves the same way Tauri expects.
echo "▸ starting vite..."
bun run dev > /tmp/scrcpy-vite.log 2>&1 &
VITE_PID=$!

# Wait for vite to listen on its port before launching the app.
for _ in $(seq 1 30); do
  if curl -sf "http://localhost:$VITE_PORT/" -o /dev/null; then break; fi
  sleep 0.3
done
if ! curl -sf "http://localhost:$VITE_PORT/" -o /dev/null; then
  echo "❌ vite never became ready on :$VITE_PORT — see /tmp/scrcpy-vite.log"
  cat /tmp/scrcpy-vite.log
  cleanup
fi
echo "  vite ready on :$VITE_PORT"

# 4. Launch the signed binary. TAURI_DEV_WATCHER_IGNORE keeps Tauri's own
#    file watcher quiet (we don't use it — we own the lifecycle).
echo "▸ launching scrcpy-mac-ui (signed)..."
TAURI_DEV_HOST="localhost" "$BIN" &
APP_PID=$!
echo "  app pid: $APP_PID"

echo ""
echo "Dev session running. Ctrl+C to stop."
echo "  • Edit Rust → re-run ./scripts/dev.sh"
echo "  • Edit TS/CSS → vite HMR picks it up automatically"
echo ""

# Wait on the app process so Ctrl+C reaches the trap.
wait "$APP_PID"
cleanup
