# Magpie — briefing for Claude sessions

You're picking up Magpie mid-flight. Read this whole file before touching
anything: it's the shared memory of every session that has worked on this
repo, written so you can carry on as if you'd built it yourself. Machine-
specific notes for the maintainer's own computer live in `CLAUDE.local.md`
(git-ignored); read it too if it exists.

If you run alongside other sessions, follow **Working with other sessions**
below. If you learn something the next session needs, or a decision changes,
update this file in the same commit.

---

## What Magpie is

A local-first personal finance desktop app: dashboard, ledger, budgets,
reports, accounts, recurring transactions, savings goals, receipts,
multi-currency with ECB rates. Native Rust (egui/eframe), one SQLite file,
no account, no telemetry. It's the GUI successor to **Pear**
(github.com/ahaan-shah/pear), the maintainer's earlier Go TUI expense tracker.
Magpie is a clean rewrite, not a port; Pear only lives on as an optional
import (Settings → Your data → Import from Pear).

Repo: github.com/ahaan-shah/magpie. Name on crates/AUR/Homebrew is
`magpie-finance`; the binary is `magpie`.

The bar the maintainer set, and holds every change to: **blazingly fast,
beautiful, light on RAM, fluid animations, robust (it must never crash),
easy to install on Linux, macOS and Windows**.

## How the maintainer works (read this twice)

- **Nothing outward-facing without an explicit go.** Releases (tags), making
  things public, publishing packages, force-pushes: only when asked in so
  many words. Approval for one release doesn't carry to the next.
- **They test before you release.** Build it, install it locally (see
  `CLAUDE.local.md`), let them try it, iterate, *then* tag. CI release builds
  are slow, so don't use them as the test loop.
- **Versions:** patch releases for features and fixes; they name the
  version ("ship this as 0.2.1").
- **Never touch their real data.** It lives in `~/.local/share/magpie/`. Test
  with `--demo`, `--data-dir`, or the test launchers. Back up the database
  (`sqlite3 … ".backup …"`) before anything that could migrate it. Don't close
  or kill a running Magpie without asking: it might have half-typed input.
- **Design taste:** clean, calm, modern, subtle. Prompts and highlights
  should inform without demanding attention (the sidebar update prompt was
  made deliberately quiet after "too big and bulky"). Accent colour is used
  sparingly. Error text is plain and human, not red stack traces:
  "Couldn't check. Make sure you are connected to the internet."
- **Wording:** short, plain, friendly. They often specify exact strings; use
  them verbatim.
- **They send follow-ups mid-task.** Fold them into the current work
  rather than starting over, and say how each was handled.
- **Report honestly**, including your own mistakes, and say what you
  actually verified (rendered, tested, ran) and what you didn't.

## Architecture

```
crates/core  (magpie-core)    no UI code; all data, maths, IO; heavily unit-tested
crates/app   (magpie-finance) the eframe app; binary `magpie`
```

### Core (`crates/core/src`)

| File | What it owns |
|---|---|
| `model.rs` | Account (with `icon` and `CardStyle`), Category, Txn, BudgetPlan, RecurringRule, Goal, Month, `today()` (honours `MAGPIE_TODAY`) |
| `money.rs` | `Cur([u8;3])`, currency table, `parse`/format. **Money is always `i64` minor units.** Never floats, except FX conversion. |
| `db.rs` | rusqlite (bundled), WAL, `MIGRATIONS` array keyed by `PRAGMA user_version`: append a new SQL string to migrate; never edit old ones |
| `store.rs` | `Store`: loads everything into memory at open (txns in a sorted `Vec`), writes through to SQLite synchronously, bumps `version()` on every change, undo/redo op log, `Settings` (key/value table) |
| `analytics.rs` | totals, cashflow, by-category, net worth (`net_worth_at(ends)`), `category_trend_spans`, payees, daily spend |
| `budget.rs`, `recurring.rs`, `goals.rs`, `fx.rs`, `quick.rs` (natural-language quick add), `receipts.rs` (content-addressed by blake3) | their features |
| `statement.rs` | bank-statement import: CSV/TSV/TXT (delimiter sniffing, UTF-8/16/Windows-1252), Excel/ODS (calamine), OFX/QFX; header/preamble detection, debit/credit or DR/CR columns, European amounts, month-name dates, day/month order by tightest date span, duplicate skipping |
| `io.rs` | CSV/XLSX/JSON export, `commit_import`, Pear import |
| `update.rs` | release check (GitHub `releases/latest`, or `MAGPIE_UPDATE_FEED`), `plan()` (can we replace ourselves?), `install()` (download → SHA256SUMS check → unpack with system tar/ditto → atomic swap) |
| `demo.rs` | demo workspace generator (`--demo`), default category seeding |

### App (`crates/app/src`)

| File | What it owns |
|---|---|
| `main.rs` | CLI flags, data dir, renderer choice (Windows: glow, then wgpu fallback), `fatal()` dialog, window options (vsync off, see below) |
| `app.rs` | `App` struct, frame loop, crash shield, shortcuts, sidebar (incl. update prompt), top bar, modal host, `Memo`, `open_external`/`open_url`, logo painter |
| `theme.rs` | 16 themes (8 light, 8 dark; Magpie Light/Dark are the defaults and the sidebar toggle) as token structs, crossfade, 9 bundled fonts (`FONTS`, `install_fonts`) |
| `motion.rs` | tween/appear/toggle helpers and durations (`MICRO` 0.12s, `STANDARD` 0.22, `EMPHASIS` 0.42, `CHART` 0.7) |
| `widgets/mod.rs` | buttons, cards (`card_in`, `card_scroll`), `grid_row`, inputs, `dropdown` + `option`/`option_value` (all menu rows), date picker, `spin_button`, `paint_circle_arrows`, progress |
| `widgets/charts.rs` | custom-painted charts: area (monotone cubic), grouped/stacked bars, donut, ring, sparkline, heatmap; `axis()` nice-step helper; collision-aware x labels |
| `views/*.rs` | one file per page, each with a `State` and `show(app, ui)`; Settings collects `Act`s and applies them after drawing. `basic_ledger.rs` is Basic mode's Transactions |
| `modes.rs` | Basic/Advanced wording and the miniature app preview (onboarding, Settings) |
| `forms.rs` | every modal (`Modal` enum): transaction, account, rule, goal, category, import, confirm, currency, help; `SHORTCUTS` table |
| `updater.rs` | update state machine (`Phase`) on a worker thread; check on launch only *shows* the prompt; install/restart only on click |
| `palette.rs` | Ctrl+K command palette |
| `dialogs.rs` | async native file dialogs (rfd) on a worker thread; `Purpose` enum routes results |
| `diag.rs` | breadcrumbs, stall watchdog, panic hook → `<data dir>/magpie-diagnostics.log` |
| `tour.rs`, `film.rs`, `monkey.rs`, `marks.rs` | screenshot tour, offscreen renders/demo film, random-input fuzz test, named UI rects for scripting |

### Why it's built this way (decisions and their reasons)

- **egui on glow (OpenGL), not wgpu** at runtime on Linux/macOS: about 30 MB
  less RAM, instant start. Windows adds wgpu as a fallback only.
- **Everything in memory, SQLite as the durable copy.** Frames never query
  the database. Derived data goes through `Memo<K, V>` keyed on
  `store.version()` (plus whatever else it depends on), so nothing in a frame
  is O(transactions). Keep it that way.
- **Repaint only while animating** → 0% CPU when idle. If you add animation,
  request repaints only while it runs.
- **Vsync is off, frames are capped at 144 fps** (`FRAME_CAP`). With vsync,
  a hidden Wayland window (another Hyprland workspace) blocked in
  `eglSwapBuffers` until the compositor flagged Magpie "not responding".
- **Crash shield:** each frame runs in `catch_unwind`; on panic the view
  resets and the app keeps going. Release builds use `panic = "unwind"` for
  this. Don't change it.
- **Store table scan + in-memory sort** at load (not `ORDER BY`): measured
  faster for 100k rows.
- **`axis()` for every chart scale.** An early bug computed steps from the max
  only and spun the CPU and allocated gigabytes. Use the helper.
- **Hover shading uses `t.hover_wash()` / `t.hovered(base)`**, a touch of
  the text colour. The `hover` token nearly matches cards and popups in many
  themes, so hovers drawn with it were invisible.
- **Hover by geometry** (`ui.rect_contains_pointer(rect)`) where widgets
  overlap; `resp.hovered()` caused an edit-button flicker on Accounts (there's
  a regression test).
- **File dialogs are async** on a worker thread; blocking ones froze the UI.
  `dialogs.blocked()` checks headless before creating a dialog (a macOS CI
  panic otherwise).
- **Scroll areas inside cards** consume the wheel only when they can scroll,
  so the page doesn't scroll too. Inner areas need unique `id_salt`s.
- **Logo/icons:** `scripts/make-icons.py` generates everything from one
  superellipse SVG. Linux installs **SVG only** (PNGs got upscaled and blurred
  on 2× screens), with hinted 16/24 px variants. The SVG deliberately avoids
  filters, clip paths and `<use>` so Qt, GTK and browsers draw it the same.
- **Updater gotchas:** Linux's `current_exe()` follows the renamed old
  binary after a swap, so restart uses the path captured at launch. Windows
  zips are unpacked with the `zip` crate, not `tar` (Git's GNU tar on PATH
  misreads `C:\`, and a console child flashes a window).
- **Updates check automatically but never install on their own.** The user
  clicks Update, then Restart. Package-manager installs get a download link
  (`Plan::Manual`).
- **Basic and Advanced modes** (`Settings.basic`, saved as `mode`; missing
  means Advanced so existing users see no change). One app, one data set:
  Basic is a view. `Page::nav(basic)` / `in_basic()` decide the sidebar,
  `App::go` redirects Advanced-only pages to Home, and views branch on
  `app.basic()` (Home in `dashboard::show_basic`, a separate
  `basic_ledger`, smaller forms). `Ctrl Shift T` (and Settings) toggle via
  `set_basic`, which fades the whole app out, switches while hidden
  (`apply_basic`, snapping the sidebar), and fades back in. Check it before
  `Ctrl T` (egui lets Ctrl+T match Ctrl+Shift+T). Anything
  Advanced creates (transfers, other currencies, rollover, tags) must still
  read sensibly in Basic, and hidden form fields keep their values.
  New features: decide if they belong in Basic, and keep Basic small.
- **Paybacks are refunds:** money in filed under an *expense* category
  lowers that category's spending (`analytics::flow`), so budgets,
  "Where it went" and totals all net it, and it never counts as income. The
  transaction form's "Someone's paying me back" toggle just offers expense
  categories for money in; there's no extra field in the database.
- **What's new after updates:** after an update, the sidebar's update spot
  shows "What's new" (a gift with an accent dot). It opens a card of the
  release's points (`whatsnew.rs`); moving to any other page puts the row
  away for good (`App::go` → `dismiss`), and Settings → About links to the
  release page. "Just updated" comes from the updater's restart
  (`MAGPIE_UPDATED_FROM`) or the saved `seen_version` being older; with
  neither, an onboarded workspace (an update from ≤ 0.2.2) sees the current
  notes and a new one sees nothing. Demo workspaces start as seen.
- **Licences:** About just says "MIT licensed". `THIRD-PARTY-LICENSES.txt`
  (generated by `scripts/third-party-licenses.py`) ships in every download
  and via `magpie --licenses`. Re-run the script when dependencies change.

### Conventions

- Rust 2024 edition, rust-version 1.88, `max_width = 120`. CI runs
  `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`,
  `cargo test --all --locked` on Linux, macOS and Windows. Run all three
  before every commit.
- Sign convention: **negative = money out.**
- New settings: add the field to `Settings`, its default, load and save in
  `store.rs`.
- New menus: use `w::option` / `w::option_value`, never
  `selectable_label`/`selectable_value` (consistent look, scrollbar lane).
- Dates: `jiff::civil::Date`. Months: `Month`.
- Keyboard: every feature gets a shortcut or palette entry; update
  `forms::SHORTCUTS` and the README table.
- Match the surrounding code's comment density and idiom; comments explain
  *why*.
- Commits end with the Co-Authored-By line your harness specifies.

## Dev loop

```bash
cargo run --profile fast -- --demo         # quick optimized build + sample data
cargo test --all                            # ~45 s (the monkey test is the slow one)
MAGPIE_STILL_DIR=target/tmp/stills cargo test --profile fast -p magpie-finance stills -- --ignored
                                            # renders UI states to PNGs (dropdown, onboarding, update prompt) — look at them
MAGPIE_TOUR=target/tmp/tour MAGPIE_ZOOM=0.6 MAGPIE_TODAY=2026-09-29 cargo run --release -- --demo
                                            # screenshots of every page (README images come from this)
```

Env switches: `MAGPIE_DATA_DIR`/`--data-dir`, `MAGPIE_TODAY`, `MAGPIE_ZOOM`,
`MAGPIE_DEBUG`, `MAGPIE_HEADLESS` (no dialogs, network or launching apps),
`MAGPIE_EXPORT_DIR`, `MAGPIE_TOUR`, `MAGPIE_RENDERER=glow|wgpu` (Windows),
`MAGPIE_UPDATE_FEED` (URL or local JSON file shaped like GitHub's
releases/latest; asset URLs may be local paths, for testing updates offline).

**Always look at your UI changes.** Extend the `stills` test (in `film.rs`)
to render the state you changed, view the PNG, then show the maintainer in
the real app. Release builds use fat LTO and take minutes; use
`--profile fast` while iterating.

## Releasing (only when asked)

1. Bump `version` in `Cargo.toml` (workspace) and `crates/app/Cargo.toml`'s
   `magpie-core` dep, `packaging/aur/PKGBUILD`, `packaging/homebrew/*.rb`,
   README's `MAGPIE_VERSION=` example; `cargo update -w`.
2. CHANGELOG entry (the release notes are cut from it automatically).
3. **What's new notes** (every release from 0.2.3 on): add an entry for the
   new version at the top of `whatsnew::RELEASES` in
   `crates/app/src/whatsnew.rs`. It's a shorter, simpler CHANGELOG: 2–4
   points, each an icon, a few-word title and one everyday sentence on what
   changed and why it helps (no jargon, no internals; fixes only if people
   would notice). Add a picture only when it really shows the change: a
   ~2:1 PNG in `crates/app/assets/whatsnew/`, rendered from the stills, via
   `include_bytes!`. A test fails if the version has no notes. Render the
   card (`basic_stills` → `whatsnew-card`) and look at it.
4. fmt / clippy / test, commit, push `main`, then
   `git tag -a vX.Y.Z -m "Magpie X.Y.Z" && git push origin vX.Y.Z`.
5. `release.yml` builds Linux x86_64 + aarch64 tarballs, a macOS universal
   .dmg/.zip, and (from 0.2.0) the Windows zips and installer. It smoke-tests
   the Windows installer, writes SHA256SUMS, and publishes. Watch it with
   `gh run watch`. A manual `workflow_dispatch` run builds everything
   without publishing.
6. Asset names are a contract with `install.sh`, `install.ps1` and the
   in-app updater (`update::asset_name`). Don't rename them.

Install paths for users: `install.sh` (curl one-liner) for Linux/macOS and
`install.ps1` (`irm … | iex`) or the setup .exe for Windows are the
documented methods. AUR, Homebrew and `cargo install` were pulled
from the README until they're actually published. The PKGBUILD and cask in
`packaging/` are ready but unpublished.

## Where things stand

- **On `main`, not yet released (next: 0.2.3):** What's new after updates.
  Its 0.2.3 notes in `whatsnew::RELEASES` currently list only that; add
  each new 0.2.3 feature as a point there (and in CHANGELOG → Unreleased).
- **Released:** up to v0.2.2 (paybacks). v0.2.1 brought Basic mode, the
  Magpie themes and database migration v2 for account card looks. v0.2.0 was the first release with
  Windows (0.1.5 was folded into 0.1.6). The repo was private until 0.1.6, then made public.
- **Windows:** Inno Setup installer (`packaging/windows/magpie.iss`,
  x64+ARM64 in one, per-user, upgrades in place, keeps data on uninstall)
  and `install.ps1`. The release job smoke-tests the installer, checks the
  zips' layout, and runs the `install.ps1` one-liner under Windows
  PowerShell 5.1 against a local mirror. Unsigned for now (Azure Trusted
  Signing later). An in-app update on Windows swaps `magpie.exe` but leaves
  the version shown in Settings → Apps at the installed one until the next
  installer run.
- **Later, not started:** publishing to AUR (`magpie-finance` and a
  `-bin`) and a Homebrew tap (`ahaan-shah/homebrew-tap`), with CI bumping
  them per release; Apple notarization (needs a Developer account); code
  signing on Windows.
- **Not done on purpose:** no PDF statement import (unreliable); no cloud
  sync; no telemetry.

## Working with other sessions

Several Claude sessions may work on this repo at once. To stay out of each
other's way:

1. **Use your own worktree and branch:**
   `git worktree add ../magpie-<topic> -b agent/<topic>` (or the harness's
   worktree feature). Never commit straight to `main` while others are
   active.
2. **Claim work on the board.** `docs/agents/BOARD.md` lists who's doing
   what. Before starting, add a line with your topic, branch, and the files
   or areas you expect to touch; check nobody else holds them. Update it
   when you finish or hand off. Commit board changes to `main` on their own,
   small and quick, so others see them.
3. **Hot files:** `app.rs`, `forms.rs`, `widgets/mod.rs`, `store.rs` and
   `settings.rs` are touched by most features. Keep edits there small and
   localized, and rebase often (`git fetch && git rebase origin/main`).
4. **One release owner.** Only the session the maintainer asked to release
   bumps versions, edits the CHANGELOG header, tags, or edits `release.yml`.
   Others add their CHANGELOG lines under `## [Unreleased]` (create it at the
   top if missing).
5. **Database migrations** are append-only. Claim the next index on the
   board before writing one; two sessions adding "migration 4" collide.
6. **Merging:** rebase on `main`, run fmt, clippy and test, fast-forward
   merge, then remove your worktree. No force-pushes to `main`, ever.
7. **Shared knowledge goes here.** New architecture, a decision, a gotcha:
   edit this file in the same PR. Keep it a briefing, not a diary.
