# Changelog

All notable changes to Magpie are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.1.3] - 2026-10-01

### Added
- Reports: "This month", 3, 6 and 12 months, year to date, and a custom
  range with from and till dates (till follows today unless you set it).
  Periods of about a month chart by day, about a quarter by week, longer by
  month; averages switch between per day and per month to match.

### Changed
- The logo is a plain-vector SVG on Linux: no PNGs, so it's sharp at any size
  and display scale (docks and app switchers on 2× screens were upscaling a
  bitmap). It avoids filters and clip paths, so Qt, GTK and browsers all draw
  it identically, and has hinted versions for 16 and 24 px. The installer
  removes PNG icons left by older versions.
- Chart axis labels thin themselves out instead of overlapping.
- A savings rate below -100% reads "Spent 2.7× income" instead of "-170%".

## [0.1.2] - 2026-10-01

### Added
- Font picker (Settings → Appearance and feel): Inter, Geist, Onest, Plus
  Jakarta Sans, DM Sans, Figtree, Outfit, IBM Plex Sans and JetBrains Mono,
  all bundled. Switches instantly.
- A demo-film renderer that records the real app frame-by-frame with
  scripted cursor, typing and camera moves (`crates/app/src/film.rs`).

### Changed
- A new high-resolution squircle logo.
- In the command palette, "search transactions for …" always ranks last, so
  typing a theme or page name picks it.

### Fixed
- Savings-rate subtitles no longer show absurd percentages early in a month.

## [0.1.1] - 2026-10-01

### Added
- Seven new themes from the Omarchy palettes, balanced 7 light / 7 dark:
  Latte, Flexoki, Rosé Pine Dawn, Lupine, Snow, Kanagawa and Everforest.
  Settings groups themes into Light and Dark.
- Scroll speed slider in Settings → Appearance and feel.

### Changed
- The Magpie logo and app icon are now brand orange.
- The sidebar's collapse button is a rounded « / » and lines up with the
  other icons when the sidebar is compact.

### Fixed
- Text no longer runs off cards in smaller windows (goals, dashboard,
  recurring, reports and budget rows shorten with "…"); goal cards scale
  their ring to fit; the Transactions filters and table stay on screen.
- The month ring no longer shows nonsense like "-5853% saved" early in a
  month.

## [0.1.0] - 2026-10-01

First release.

### Added
- Dashboard: net worth, this month, KPI tiles with sparklines, cash flow,
  category donut, budgets, upcoming bills, recent activity, goals and a
  month-in-review card.
- Transactions: natural-language quick add, filters and search, a
  virtualized ledger grouped by day, multi-select bulk edits, undo/redo,
  receipts (drag and drop) and CSV import with column mapping.
- Budgets with rollover, pacing and "fit to 3-month average".
- Reports: cash flow, net worth, category trends, top payees and a spending
  calendar.
- Multiple accounts and transfers, multi-currency with daily ECB rates.
- Recurring transactions with auto-post or confirm-first.
- Savings goals with progress rings and projected finish dates.
- 8 themes with animated crossfade, command palette (Ctrl+K), keyboard
  shortcuts.
- Export to CSV, Excel and JSON; database backups; import from Pear.
- Adjustable interface size, keyboard shortcuts for everything (press `?`),
  undoable budget edits, custom budget categories, and main-currency
  switching that converts budgets, goals and (optionally) accounts.
- Crash shield and diagnostics log; headless "monkey" stress test in CI.
