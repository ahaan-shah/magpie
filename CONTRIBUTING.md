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

## Handy switches

- `MAGPIE_DEBUG=1` — print time-to-first-frame and any slow frames (>8ms).
- `MAGPIE_TOUR=<dir>` — visit every page and save screenshots, then quit
  (used for the README images). Combine with `MAGPIE_ZOOM=0.62` for a wide
  layout on a small screen.
- `MAGPIE_DATA_DIR=<dir>` or `--data-dir <dir>` — use a different database.
- `cargo run --release -p magpie-core --example bench 100000` — load-time
  benchmark with 100k transactions.

## Guidelines

- Money is always `i64` minor units — never floats (conversion aside).
- Nothing in a frame should be O(number of transactions): derive data in a
  `Memo` keyed on `Store::version()`.
- Animations go through `motion.rs` and must stop requesting repaints when
  they finish, so an idle Magpie uses 0% CPU.
- Keep PRs focused, with a line in `CHANGELOG.md` for user-visible changes.
