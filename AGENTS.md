# Repository Guidelines

## Project Structure & Module Organization
- `src/` hosts the Vite + TypeScript frontend. Place assets under `src/assets/` and keep logic thin; heavy lifting lives in Rust.
- `src-tauri/` contains the Rust backend. Keep shared helpers in `assistant_lib` and add integration tests under `src-tauri/tests/`.
- `src-tauri/src/soniox.rs` manages Soniox real-time transcription; `src-tauri/src/utils.rs` centralizes logging and cross-cutting helpers.
- Configuration files live in `config/`; never commit secrets. Built artifacts land in `dist/` and are consumed by Tauri.

## Build, Test, and Development Commands
- `npm run dev` starts the Vite dev server for quick frontend iteration.
- `npm run tauri dev` launches the Tauri desktop app with hot reload across Rust and frontend layers.
- `npm run build` type-checks via `tsc` and outputs the production bundle to `dist/`.
- `cargo test --manifest-path src-tauri/Cargo.toml` executes Rust unit and integration tests.
- `npm run tauri build` packages the desktop application; ensure prior steps pass.

## Coding Style & Naming Conventions
- TypeScript/CSS use 2-space indentation and lowercase-hyphenated filenames (e.g., `audio-recorder.ts`).
- Rust follows rustfmt defaults; modules/functions in `snake_case`, types in `CamelCase`.
- Group imports logically (std, third-party, local) and colocate small helpers near usage. Run `cargo fmt` and `cargo clippy` before submitting work.

## Testing Guidelines
- Maintain deterministic Rust tests in `src-tauri/tests/`; prefer asserting on file presence or sizes over raw byte equality.
- Soniox audio helpers include unit tests (`test_render_tokens_basic`, `test_to_pcm_identity_when_16k_mono`, etc.); extend them when adding audio transforms.
- Frontend lacks a formal harness—push testable logic into Rust and validate manually with `debug_soniox.js` when touching WebSocket flows.

## Commit & Pull Request Guidelines
- Use concise, imperative commit subjects (e.g., `Add MP3 encoding support`). Group related changes into cohesive commits.
- PRs should describe the change, link issues, outline reproduction steps, and include platform-specific notes or UI screenshots.
- Confirm `cargo fmt`, `cargo clippy`, and `npm run build` all succeed before requesting review.

## Security & Configuration Tips
- Store Soniox credentials in `config/soniox.local.json` (gitignored) and never embed keys in source.
- The Tauri identifier is `com.pawel.assistant`; keep it consistent across configuration files.
- Debug logs write to `~/Documents/vad_debug.log`. Inspect them when diagnosing audio, VAD, or transcription issues.
