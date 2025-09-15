# Repository Guidelines

## Project Structure & Module Organization
- `src/` – Frontend (TypeScript, Vite). Assets under `src/assets/`.
- `src-tauri/` – Backend (Rust, Tauri). Library `assistant_lib`, binaries, config, and tests in `src-tauri/tests/`.
- `src-tauri/src/soniox.rs` – Soniox API integration for real-time speech-to-text transcription.
- `src-tauri/src/utils.rs` – Utility functions including debug logging system.
- `config/` – Configuration files (Soniox API credentials and settings).
- `dist/` – Built frontend artifacts consumed by Tauri.
- `debug_soniox.js` – Browser console debugging tool for Soniox integration testing.
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
- Soniox module includes unit tests for audio processing functions (`test_render_tokens_basic`, `test_to_pcm_identity_when_16k_mono`, etc.).
- Prefer small, deterministic tests for audio/encoding logic; assert file creation/size over brittle byte-for-byte equality.
- Frontend currently has no test harness; keep logic minimal and push heavy lifting to Rust where it's testable.
- Use `debug_soniox.js` in browser console for manual testing of Soniox integration and event flow.

## Commit & Pull Request Guidelines
- Commits: imperative, concise subject (e.g., "Add MP3 encoding support"); group related changes.
- PRs: clear description, linked issue(s), reproduction steps; include screenshots/gifs for UI changes and platform notes (macOS/Windows/Linux).
- CI hygiene: code formatted (`cargo fmt`), clippy clean, frontend builds locally (`npm run build`).

## Security & Configuration Tips
- Do not commit secrets or OS-specific paths. Tauri identifier is `com.pawel.assistant` (see `src-tauri/tauri.conf.json`).
- Use `invoke` only for vetted commands; validate inputs crossing TS↔Rust boundaries.
- Soniox API credentials should be stored in `config/soniox.local.json` (gitignored) - never commit API keys.
- Debug logs are written to `~/Documents/vad_debug.log` for troubleshooting audio and transcription issues.

## Audio & Transcription Architecture Notes
- The application supports dual recording modes: manual recording and Voice Activity Detection (VAD).
- Soniox integration works in both modes by streaming continuous audio for optimal speech recognition context.
- Audio streams are handled differently: manual recording uses `build_stream_*` functions, VAD uses `process_chunk` closure.
- Both paths include Soniox audio transmission for consistent transcription functionality.
- Audio format conversion ensures compatibility: F32→I16, U16→I16, resampling to 16kHz mono PCM for Soniox.
- WebSocket connection managed asynchronously with Tokio for real-time streaming without blocking audio processing.
