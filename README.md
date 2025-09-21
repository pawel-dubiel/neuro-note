# Neuro Note

Neuro Note captures conversations, transcribes them in real time, and routes the transcript through configurable AI assistants. The project favours predictable behaviour: missing configuration is treated as an error and surfaces immediately.

## Capabilities

- **Consistent audio capture** – Manual sessions create timestamped files while voice-activated mode arms a VAD loop that writes only when speech is detected; both paths share the same pause/resume controls and state machine.
- **Format and quality control** – Record to WAV or MP3, pick MP3 encoding quality, and rely on buffered LAME encoding so sessions flush cleanly on stop.
- **Live transcription** – Streams microphone audio to Soniox for sentence-by-sentence updates, language identification, diarization, and optional translation; the UI keeps tentative text separate from confirmed results.
- **Transcript-aware AI analysis** – Maintains multiple assistant profiles, runs gating checks before calling a main model, tracks model vs gate invocations, and keeps a scrollable history of answers.
- **Provider flexibility** – Switch between OpenAI and OpenRouter at runtime, fetch model lists after keys are entered, and display OpenRouter credit usage without caching stale data.
- **Transparent operations** – On-screen meters show input levels, every state change is emitted to the UI, and detailed logs land in `~/Documents/vad_debug.log` for troubleshooting.

## Getting Started

1. Install a recent Node.js (for the UI tooling) and the Rust toolchain (for the native backend).
2. Install dependencies with `npm install`.
3. Duplicate `config/config.example.json` to `config/config.local.json` and fill in the sections you plan to use. Missing keys cause explicit errors; do not leave placeholders.
4. (Optional) Copy `config/soniox.example.json` to `config/soniox.local.json` with your Soniox key to enable live transcription.
5. (Optional) Edit `config/assistants.json` to define assistant profiles, system prompts, and output policies.
6. Start the UI with `npm run dev` or launch the desktop shell with `npm run tauri dev`.

## Runtime Overview

- Choose manual recording to pick a save path immediately, or enable the voice detector to wait for speech before writing audio.
- The pause/resume controls work for both modes and keep the state machine in sync with the UI indicators.
- When transcription is enabled, the app opens a websocket session, forwards 16 kHz PCM frames, and emits transcript deltas back to the interface.
- AI analysis runs only after the gate model returns `run=true`, at which point the selected assistant template renders the user prompt and calls the provider model. Results are logged and stored in the client-side history stack.

## Configuration Notes

- `config/config.local.json` holds runtime toggles: recording defaults, provider selection, and API keys. The loader refuses to start if required sections are missing.
- `config/assistants.json` defines assistant metadata. Empty IDs, prompts, or names raise errors during load to avoid falling back to undefined behaviour.
- `config/soniox.local.json` is only needed when transcription is active; the UI warns and refuses to start a session if the key is missing.
- UI changes persist through the config modal by calling `save_app_config`, so keep the file writable during development.

## Plans

1. Ship the multi-stage pause/resume refactor outlined in `TODO.md`, including atomic state transitions and dedicated command processing.
2. Separate audio capture, buffering, and file writing threads to remove locking contention and improve failure recovery.
3. Extend the automated test suite with stress cases for rapid state flips, transcript gating, and audio encoder flush behaviour.
4. Polish the voice detection UX with clearer status messaging and shortcuts once the new state machine is in place.

## Diagnostics

- Streaming and AI activity is logged via `utils::log_to_file`; inspect `~/Documents/vad_debug.log` when debugging voice detection or API calls.
- Use `cargo test --manifest-path src-tauri/Cargo.toml` to run backend tests and `npm run build` for TypeScript type checks.
- If the UI shows `Gate: -` or `Credits: ?`, review API keys and rerun `load_assistants` from the config modal; the app will not fall back to anonymous requests.
