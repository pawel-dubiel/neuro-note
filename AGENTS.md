# Repository Guidelines

## Project Structure & Module Organization
- `src/` – Frontend (TypeScript, Vite). Assets under `src/assets/`.
- `src-tauri/` – Backend (Rust, Tauri). Library `assistant_lib`, binaries, config, and tests in `src-tauri/tests/`.
- `dist/` – Built frontend artifacts consumed by Tauri.
- `index.html`, `vite.config.ts`, `tsconfig.json` – Frontend entry and tooling.

## Build, Test, and Development Commands
- `npm run dev` – Run Vite dev server for the web frontend.
- `npm run tauri dev` – Launch the desktop app with live reload (frontend + Rust).
- `npm run build` – Type-check (`tsc`) and build the frontend to `dist/`.
- `npm run preview` – Serve the built frontend locally.
- Rust tests: `cargo test --manifest-path src-tauri/Cargo.toml`.
- Package app: `npm run tauri build`.

## Coding Style & Naming Conventions
- TypeScript/CSS: 2-space indentation; prefer lowercase file names (`main.ts`, `styles.css`), hyphenate multiword names (`audio-recorder.ts`).
- Rust: follow rustfmt defaults; snake_case for functions/modules, CamelCase for types. Run `cargo fmt` and `cargo clippy` before PRs.
- Imports: use relative paths within `src/`; group std/3rd-party/local in Rust.
- Avoid magic numbers; co-locate small helpers near usage.

## Testing Guidelines
- Rust integration tests live in `src-tauri/tests/*.rs` (e.g., `audio_writer_tests.rs`). Run with `cargo test --manifest-path src-tauri/Cargo.toml`.
- Prefer small, deterministic tests for audio/encoding logic; assert file creation/size over brittle byte-for-byte equality.
- Frontend currently has no test harness; keep logic minimal and push heavy lifting to Rust where it’s testable.

## Commit & Pull Request Guidelines
- Commits: imperative, concise subject (e.g., "Add MP3 encoding support"); group related changes.
- PRs: clear description, linked issue(s), reproduction steps; include screenshots/gifs for UI changes and platform notes (macOS/Windows/Linux).
- CI hygiene: code formatted (`cargo fmt`), clippy clean, frontend builds locally (`npm run build`).

## Security & Configuration Tips
- Do not commit secrets or OS-specific paths. Tauri identifier is `com.pawel.assistant` (see `src-tauri/tauri.conf.json`).
- Use `invoke` only for vetted commands; validate inputs crossing TS↔Rust boundaries.
