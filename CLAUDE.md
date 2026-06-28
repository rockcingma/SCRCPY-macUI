# scrcpy-mac-ui

A native macOS controller for scrcpy. Tauri 2 (Rust) + React/TypeScript.
See [docs/PRD.md](docs/PRD.md) for the full spec and design decisions.

## Testing

100% coverage is the goal — tests make vibe coding safe.

- **Frontend:** `bun run test` (Vitest + @testing-library/react). Tests in `src/test/`.
- **Backend:** `cargo test` from `src-tauri/`. Rust unit tests are inline `#[cfg(test)]` modules.

Test expectations:
- New function → corresponding test.
- Bug fix → regression test.
- New error path / conditional → test both branches.
- Never commit code that breaks existing tests.

Pure logic (device-state derivation, settings, arg-building, validation) is split
from side-effecting code (Tauri IPC, process spawn) so it is testable without mocks.

## Architecture

- `src/` — React frontend. `types.ts` mirrors the Rust `AppError` union and presets.
  `backend.ts` is the injectable IPC interface (real impl lazy-imports Tauri).
- `src-tauri/src/` — Rust backend. `adb.rs` (device parsing + PATH probe),
  `scrcpy.rs` (spawn + async stdout/stderr drain + kill ladder + input validation),
  `error.rs` (serde-tagged AppError), `lib.rs` (Tauri commands).

## Skill routing

When the user's request matches an available skill, invoke it via the Skill tool. When in doubt, invoke the skill.

Key routing rules:
- Product ideas/brainstorming → invoke /office-hours
- Strategy/scope → invoke /plan-ceo-review
- Architecture → invoke /plan-eng-review
- Design system/plan review → invoke /design-consultation or /plan-design-review
- Full review pipeline → invoke /autoplan
- Bugs/errors → invoke /investigate
- QA/testing site behavior → invoke /qa or /qa-only
- Code review/diff check → invoke /review
- Visual polish → invoke /design-review
- Ship/deploy/PR → invoke /ship or /land-and-deploy
- Save progress → invoke /context-save
- Resume context → invoke /context-restore
