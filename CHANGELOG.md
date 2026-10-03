# Changelog

All notable changes to Magpie are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Basic mode: a simpler Magpie for everyday money. Four pages (Home,
  Transactions, Budgets, Accounts), plainer words, and a shorter Add
  transaction form (amount, what it was for, category, date). Everything
  else is Advanced mode, which is Magpie as it was. Switch any time in
  Settings → App mode or with `Ctrl Shift T`; both modes show the same data.
- New users choose Basic or Advanced when they first open Magpie, with a
  preview of each.
- Make account cards your own: besides the colour, pick an icon and a look
  (Simple, Tinted or Bold), with a live preview as you edit.
- A Magpie theme in light and dark, in the brand's soft orange, beige,
  green and blue. The sidebar's light/dark button now switches between them,
  and new installs start in Magpie Dark.

### Changed
- Creating a category that already exists (in any capitalisation) uses the
  existing one instead of showing an error.

### Fixed
- Hovering a row in dropdowns and menus shades it again; the shade was
  nearly invisible in most themes. Buttons and list rows hover more clearly
  too.
- "New category…" in Budgets → Add budget no longer gets cut off.
- Dropdowns open just below their field instead of overlapping it, and
  always fit in the window (they scroll when there isn't room).
- The transaction details panel's scrollbar sits in its margin instead of
  over the icons and the receipts box.
- With the details panel open, the ledger no longer prints an "Account"
  header over the amounts when that column is hidden.
- `install.sh --uninstall` (`curl … | sh -s -- --uninstall`) removes Magpie
  on Linux and macOS and keeps your data.

## [0.2.0] - 2026-10-02

Magpie now runs on Windows. Everything in 0.1.6 is here, on all three
platforms.

### Added
- Windows 10 (1809+) and 11, on x64 and ARM64. Install with one line in
  PowerShell (`irm https://raw.githubusercontent.com/ahaan-shah/magpie/main/install.ps1 | iex`),
  or download `Magpie-windows-setup.exe` or a portable zip from the release.
  The installer sets Magpie up just for you with no admin prompt, adds it to
  the Start menu, upgrades in place, and keeps your data when you uninstall.
- On Windows, Magpie draws with OpenGL and switches to Direct3D by itself on
  PCs where OpenGL can't start (virtual machines, remote desktop, some older
  laptops).
- In-app updates work on Windows too.

### Fixed
- The arrows on "Check for updates" turn at a steady speed instead of
  rushing, slowing and rushing again, and come to rest smoothly.
- Restart after an in-app update now reopens Magpie on Linux. It used to
  just close it.
- In-app updates unpack Windows downloads themselves, so a different `tar`
  on the PATH (from Git, say) can't break them, and retry briefly if
  antivirus is still scanning the new version.

## [0.1.6] - 2026-10-02

### Added
- Updates: Magpie checks for a new release on launch (Settings → About →
  "Automatically check for updates", on by default) and with "Check for
  updates". When one is out, an "Update available" prompt appears in the
  sidebar and in Settings. Nothing downloads until you click Update: Magpie
  then fetches the build for your system, verifies its checksum, installs
  it in place (your data is untouched) and offers a Restart. Copies
  installed by a package manager point you to the download instead.
- `magpie --licenses` prints the licenses of the bundled fonts, icons and
  libraries, which now also ship as THIRD-PARTY-LICENSES.txt in every
  download.
- Import bank statements in whatever format the bank gives you: CSV, TSV or
  text (comma, semicolon, tab or pipe separated; UTF-8, UTF-16 or
  Windows-1252), Excel (.xlsx, .xls), OpenDocument (.ods) and OFX/QFX.
  Magpie finds the header under any account-details preamble, ignores
  totals rows, understands separate debit/credit columns or a DR/CR column,
  European amounts ("1.234,56"), "12.50 DR", and dates like "12 Mar 2026",
  and works out day/month order from the dates themselves.
- Re-importing an overlapping statement skips transactions that are
  already in the account.
- Onboarding: "Import data" brings in a statement right away, and keeps the
  account at the balance you entered.

### Changed
- Settings → About is just "Local-first personal finance. MIT licensed."
- Onboarding: "A calm nest for your money." (in italics) over "All your
  data, on your computer.", the account name is a placeholder you just type over, and
  "Get started" lost its arrow. The Pear import moved to Settings only.
- A custom `--data-dir` or `--demo` workspace keeps its own window state.

## [0.1.4] - 2026-10-02

### Changed
- Dropdowns and menus: the current choice gets a soft tint and a check mark
  instead of a heavy accent block, rows have a gentle hover, and the list
  keeps a lane clear for the scrollbar so it never overlaps a highlight.
- README: install instructions are just the one-line script for now.

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
