# CashGear FotoEditor instructions

## Workflow

- Work directly on main, as explicitly selected by the user. Use small conventional
  commits. Every commit must build and pass its applicable checks.
- Work one phase at a time. Before a new phase, read docs/PLAN.md,
  docs/PROGRESS.md, and docs/DECISIONS.md; ask its unresolved product questions.
  Existing user decisions persist; do not ask for them again.
- When a phase passes all acceptance criteria and remote CI is green, create and
  push its completion tag. P0 uses p0-scaffold.
- Finish with a phase report covering shipped work, deferred work, measured
  performance, and the proposed next phase. Stop before starting that next phase.
- Do not spawn agents unless the user explicitly requests delegation.

## Architecture

- Pixel processing belongs in Rust. TypeScript must never loop over document pixels.
- The five processing crates remain independent of browser, UI, and GPU types.
- The engine worker owns document state and GPU resources. UI state belongs in React/zustand.
- Every document mutation must use a command; implement command/undo foundations
  before the first painting feature.
- Preserve exact straight-alpha document samples. Premultiply temporary display
  and compositing buffers as required.
- Account for document, history, temporary, and GPU residency from the tile-store design.
- The v1 desktop app uses Tauri 2 to host the same WASM engine in WebView2.
- New dependencies must use approved free licenses and be recorded in docs/LICENSES.md.
  MPL-2.0 is explicitly allowed file-level copyleft, not a permissive license.

## Verification

Run formatting, clippy (native and WASM), Cargo tests, TypeScript checks, ESLint,
Vitest, production build/size measurement, and Playwright on both browser backends.
P0 also requires the minimal Windows/WebView2 shell to build and launch.
Never report software rendering as hardware performance or skip a missing backend
while declaring its acceptance test passed.
