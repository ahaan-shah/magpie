# Contributing to Magpie

Thanks for helping! Magpie is a small Rust workspace:

| Crate | What's in it |
|---|---|
| `crates/core` (`magpie-core`) | Data model, SQLite persistence, analytics, budgets, recurring, FX, import/export. No UI code; heavily unit-tested. |
| `crates/app` (`magpie-finance`, binary `magpie`) | The egui/eframe desktop app: theme, motion, widgets, charts and views. |

## Getting started

```bash
cargo run -- --demo          # throwaway workspace full of sample data
cargo test                   # core + app unit tests
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

On Debian/Ubuntu you'll need `libxkbcommon-dev libwayland-dev libgl1-mesa-dev`
to build.

On Windows you need the Visual Studio C++ Build Tools. `pwsh
scripts/package-windows.ps1` builds the x64 and ARM64 zips and the installer
(needs Inno Setup 6.3+), and `scripts/smoke-windows.ps1` runs the same
install-run-uninstall check that CI does.

## Handy switches

- `MAGPIE_DEBUG=1` — print time-to-first-frame and any slow frames (>8ms).
- `MAGPIE_TOUR=<dir>` — visit every page and save screenshots, then quit
  (used for the README images). Combine with `MAGPIE_ZOOM=0.62` for a wide
  layout on a small screen.
- `MAGPIE_DATA_DIR=<dir>` or `--data-dir <dir>` — use a different database
  (and window state), e.g. to try onboarding without touching real data.
- `MAGPIE_RENDERER=glow|wgpu` — Windows only: force OpenGL or Direct3D
  instead of trying OpenGL first.
- `MAGPIE_STILL_DIR=<dir> cargo test --release -p magpie-finance stills --
  --ignored` — renders a few UI states (like an open dropdown) to PNGs.
- `cargo run --release -p magpie-core --example bench 100000` — load-time
  benchmark with 100k transactions.
- `cargo test --profile fast -p magpie-finance film -- --ignored --nocapture`
  — renders a demo film (`docs/magpie-demo.mp4`, not committed) from the real
  app on a virtual clock. The script (cursor, typing, camera, captions) lives
  in `crates/app/src/film.rs`.

## Guidelines

- Money is always `i64` minor units — never floats (conversion aside).
- Nothing in a frame should be O(number of transactions): derive data in a
  `Memo` keyed on `Store::version()`.
- Animations go through `motion.rs` and must stop requesting repaints when
  they finish, so an idle Magpie uses 0% CPU.
- Keep PRs focused, with a line in `CHANGELOG.md` for user-visible changes.
