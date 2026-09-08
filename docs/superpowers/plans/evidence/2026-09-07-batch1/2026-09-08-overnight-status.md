# Overnight relay status — 2026-09-08

## Verified green

- Rust: 160 passed, 0 failed, 3 ignored (the three ignored cases are real-database drills).
- Frontend: TypeScript clean; 19 test files / 87 tests passed in the latest full run; Vite production build passed.
- Desktop packaging: worktree NSIS installer built at `src-tauri/target/release/bundle/nsis/`.
- Latest Tauri release build completed successfully after the calendar and picker changes; installer remains isolated under the worktree target directory.
- `git diff --check`: clean. The CRLF notices are Windows line-ending normalization, not whitespace errors.
- Frozen v1.3.0 artifacts were not touched. SHA-256 remains:
  - portable: `DD816B5B721B4BE9E60D791895178336AE20BECB25792F84C7B1585127CC66BD`
  - setup: `2A20C15BE2C8A6F72FD11F9D0C6A100C8D08CC87E9A2C5851A7192030658723A`

## Product work completed in this relay

- Optional task metadata is truly optional: no implicit project, tag or priority on title-only creation.
- Relationship changes use clear/set/keep semantics and revision guards; tag deletion clears associations.
- Task details now expose searchable project/tag pickers, budget editing, explicit save state and draft recovery.
- Management is one destination with task/project/tag sibling views, remembered filters and scroll position, and a compact layout for narrow windows.
- Project and tag managers have fuzzy Chinese/pinyin search.
- Searchable project/tag pickers support keyboard up/down navigation, Enter confirmation and Escape dismissal.
- Deadline entry uses a local-calendar glass picker with strict validation, Monday-first weeks, month/year navigation and today/tomorrow/one-week/clear shortcuts.
- Calendar popover also supports keyboard navigation (arrow keys, Enter to commit, Escape to close); archived current projects remain visible as read-only choices in task details.
- Deadline overdue labels refresh at local midnight and after visibility/focus recovery.
- Timer snapshots and manual/natural finish preserve a genuine tagless task round.
- Startup migration mode regression fixed: the Rust setup path now still initializes the tray while holding the database in read-only preparation mode, and the frontend renders `MigrationPrepScreen` instead of the normal shell. Added an App smoke regression covering the `MIGRATION_REQUIRED` bootstrap response.

## Still requires morning/manual verification

- Real v4 rehearsal passed against a copied `{db,-wal,-shm}` snapshot: preview was read-only; tasks 1→1, sessions 21→21, budget history 1→1 and focus segments 5→5; reopening did not repeat migration. The pre-state snapshot now copies the WAL/SHM set so committed WAL rows are included.
- Formal Tauri GUI checks remain manual: keyboard/IME, calendar interaction, window/tray close behavior, sleep/wake, scaling and visual glass treatment.
- Multi-monitor unplug/replug remains BLOCKED on this one-display machine.
- No commit, merge or push was performed; the dirty worktree intentionally contains the relay changes for the trusted terminal review.
- Latest regression gates after the fix: Rust 160 passed / 0 failed / 3 ignored; frontend 19 files / 88 tests passed; Tauri release + NSIS packaging passed.
- Installed EXE verification: the previous installed binary was an older same-version build (hash `7BCECBCE2B3B2CD1E8AD0A47853F689563FCF58863B4C02979BA0CAA7A9E3B3E`); it was preserved as `abyssal-reverie.exe.pre-fix-backup`, and the installed path now matches the verified worktree build hash `13ED65DF861A1024E59AADA79900CB2904B854912BA352FE5ED3B13C28616C9E`.

## Known non-blocking note

The pinyin search dependency increases the Vite main chunk to about 598 kB and emits the existing size warning. Search behavior is covered and green; lazy-loading can be a separate performance pass.
