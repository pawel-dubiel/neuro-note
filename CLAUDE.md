# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Tauri-based audio recording application that captures audio from the system microphone and saves it in WAV or MP3 format. The application uses a Rust backend for audio processing and a TypeScript/Vite frontend for the user interface.

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
- **Dependencies**: Audio processing via `cpal` and `hound`, MP3 encoding capabilities
- **Configuration**: `src-tauri/tauri.conf.json` - Tauri app configuration and build settings

### Key Components

**Audio Recording System**:
- Frontend initiates recording via `start_recording()` Tauri command
- Backend uses CPAL for cross-platform audio capture
- Real-time audio level monitoring with `audio-level` events
- Supports both WAV (uncompressed) and MP3 (compressed with quality settings) formats
- Files saved to user's Documents directory with timestamp naming

**State Management**:
- Rust backend maintains recording state via `AppState` struct
- Frontend manages UI state and user interactions
- Communication between frontend and backend via Tauri's invoke system and event listeners

## File Structure

- `src/` - Frontend TypeScript source code
- `src-tauri/src/` - Backend Rust source code  
- `src-tauri/Cargo.toml` - Rust dependencies and build configuration
- `src-tauri/tauri.conf.json` - Tauri application configuration
- `package.json` - Node.js dependencies and npm scripts

## Key Technologies

- **Tauri v2**: Cross-platform app framework
- **CPAL**: Cross-platform audio library for Rust
- **Vite**: Frontend build tool and development server
- **TypeScript**: Type-safe JavaScript with strict configuration
- **Hound**: WAV file I/O for Rust