# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Tauri-based audio recording application that captures audio from the system microphone, saves it in WAV or MP3 format, and provides real-time speech-to-text transcription via Soniox API. The application uses a Rust backend for audio processing and a TypeScript/Vite frontend for the user interface.

## Development Commands

- `npm run dev` - Start development server (runs Vite frontend at localhost:1420)
- `npm run build` - Build for production (compiles TypeScript and builds frontend)
- `npm run preview` - Preview production build
- `npm run tauri` - Access Tauri CLI commands
- `npm run tauri dev` - Run the full Tauri app in development mode
- `npm run tauri build` - Build the complete Tauri application for distribution

## Architecture

### Frontend (TypeScript + Vite)
- **Entry point**: `src/main.ts` - Main application logic with UI event handlers
- **Styling**: `src/styles.css` - Application styles
- **Build config**: `vite.config.ts` - Vite configuration optimized for Tauri development
- **TypeScript config**: `tsconfig.json` - Strict TypeScript configuration with ES2020 target

### Backend (Rust + Tauri)
- **Entry point**: `src-tauri/src/lib.rs` - Main Rust library with Tauri commands
- **Binary**: `src-tauri/src/main.rs` - Application entry point
- **Soniox Integration**: `src-tauri/src/soniox.rs` - Real-time speech-to-text via WebSocket
- **Utilities**: `src-tauri/src/utils.rs` - Debug logging and utility functions
- **Dependencies**: Audio processing via `cpal` and `hound`, MP3 encoding capabilities
- **Configuration**: `src-tauri/tauri.conf.json` - Tauri app configuration and build settings

### Key Components

**Audio Recording System**:
- Frontend initiates recording via `start_recording()` Tauri command
- Backend uses CPAL for cross-platform audio capture
- Real-time audio level monitoring with `audio-level` events
- Supports both WAV (uncompressed) and MP3 (compressed with quality settings) formats
- Files saved to user's Documents directory with timestamp naming
- Dual recording modes: Manual recording and Voice Activity Detection (VAD)

**Real-time Transcription System**:
- Soniox API integration for live speech-to-text transcription
- WebSocket connection for real-time audio streaming
- Works in both manual recording and voice detection modes
- Audio format conversion and resampling to 16kHz mono PCM
- Configurable via `config/soniox.local.json` (API key and settings)
- Transcript display with final and tentative text differentiation

**Voice Activity Detection (VAD)**:
- Automatic speech detection with configurable thresholds
- Dynamic noise floor calibration for improved accuracy
- Pre-roll buffering to capture speech start
- Silence detection for automatic recording stop
- Continuous file recording with voice-triggered segments

**State Management**:
- Rust backend maintains recording state via `AppState` struct
- Frontend manages UI state and user interactions
- Communication between frontend and backend via Tauri's invoke system and event listeners
- Real-time event streaming for audio levels, VAD status, and transcripts

## File Structure

- `src/` - Frontend TypeScript source code
- `src-tauri/src/` - Backend Rust source code
- `src-tauri/Cargo.toml` - Rust dependencies and build configuration
- `src-tauri/tauri.conf.json` - Tauri application configuration
- `config/` - Configuration files (Soniox API settings)
- `package.json` - Node.js dependencies and npm scripts
- `debug_soniox.js` - Browser console debugging tool for Soniox integration

## Key Technologies

- **Tauri v2**: Cross-platform app framework
- **CPAL**: Cross-platform audio library for Rust
- **Vite**: Frontend build tool and development server
- **TypeScript**: Type-safe JavaScript with strict configuration
- **Hound**: WAV file I/O for Rust
- **LAME**: MP3 encoding library
- **Soniox API**: Real-time speech-to-text transcription service
- **WebSocket**: Real-time communication for audio streaming
- **Tokio**: Async runtime for Rust WebSocket handling

## Configuration

### Soniox API Setup
1. Create `config/soniox.local.json` with your API credentials:
```json
{
  "api_key": "your_soniox_api_key_here",
  "audio_format": "pcm_s16le",
  "translation": "none"
}
```

2. Enable Soniox in the UI by checking the "Enable Soniox" checkbox
3. The system supports both manual recording and voice detection modes
4. Real-time transcripts appear in the "Transcript (live)" area

### Debug Logging
- Audio processing logs written to `~/Documents/vad_debug.log`
- Use `debug_soniox.js` in browser console for real-time event monitoring
- Available debug functions: `testSoniox()`, `checkSonioxState()`