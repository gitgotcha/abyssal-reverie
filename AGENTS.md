# Abyssal Reverie

React + Vite + Tailwind CSS frontend packaged as a Tauri 2 desktop application, with Rust and SQLite providing the authoritative business and persistence layer.

## Development

No development server is assumed to be running. Use the repository scripts from the project root:

- `pnpm tauri dev` — run the desktop application in development mode.
- `pnpm dev` — run only the Vite frontend on `127.0.0.1:1420` when desktop APIs are not required.
- `pnpm verify` — TypeScript check, Vitest suite, and Vite production build.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` — Rust tests.

Do not run a whole-repository `cargo fmt` during the management-glass work. Existing formatting debt is intentionally deferred to a separate maintenance change.

## Project Structure

Start with the task-relevant files below. Follow imports or inspect other files when required, when a documented path is missing, or when the repository contradicts this guide.

- `src/main.tsx` — React entrypoint; imports `src/index.css` and mounts `src/App.tsx`.
- `src/App.tsx` — application shell, shared frontend state, and navigation.
- `src/index.css` — global styles, fonts, Tailwind import, and application surface styling.
- `src/domain/` — frontend models and pure domain helpers.
- `src/features/` — feature UI and feature-local tests.
- `src/services/` — application gateway contracts and the Tauri implementation.
- `src/test/` — shared frontend test setup and fakes.
- `src-tauri/src/` — Rust commands, SQLite schema/migrations, repository logic, timer authority, and tests.
- `src-tauri/tauri.conf.json` — desktop packaging and application configuration.
- `vite.config.ts` — React, Tailwind CSS v4, aliases, dev-server settings, and Vitest configuration.
- `.mise.toml` — local toolchain versions for Node.js and pnpm.

## Dependencies and Styling

- Runtime: React 19 and React DOM 19.
- Styling: Tailwind CSS v4 through `@tailwindcss/vite`, plus the existing CSS in `src/index.css`.
- Desktop: Tauri 2 with Rust and SQLite.
- Build tooling: Vite 8, TypeScript 5.7, and `@vitejs/plugin-react`.
- Formatting: oxfmt for the frontend.

`src/index.css` imports Tailwind with `@import 'tailwindcss';`. Keep CSS imports first, then font declarations and global rules. Preserve the existing deep-ocean palette and background unless the active task explicitly changes that design decision.

## Reliability Boundaries

- Rust remains authoritative for timer state, task budgets, qualification, settlement, revisions, migrations, and persistence.
- Do not reproduce those rules as a second source of truth in React.
- Never run migration or corruption experiments on the user's live database. Work on verified copies and keep schema version separate from backup format version.
- Preserve unrelated user changes and frozen release artifacts. Do not overwrite `release/v1.3.0/`.

## Code Quality

- Keep JSX tags closed and braces balanced.
- Use double quotes for strings containing apostrophes, or escape the apostrophe.
- Follow the surrounding module's export style; do not mechanically convert named exports to default exports.
- For behavior changes and bug fixes, add a focused failing test first, verify the failure, implement the smallest correction, then run the relevant test and the full gate.
