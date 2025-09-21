# Gemini's Understanding of the Neuro-Note Project

This document summarizes my understanding of the "neuro-note" project based on an analysis of the codebase.

### Project Overview

The project, "neuro-note" (also referred to as "Voice Assistant Recorder"), is a desktop application built with Tauri. Its primary function is to record audio from the user's microphone and provide real-time speech-to-text transcription. The application is designed as a foundational component for future AI-powered voice assistants.

### Technology Stack

*   **Backend**: Rust with the Tauri framework (v2).
    *   **Audio**: `cpal` for cross-platform audio capture.
    *   **Audio Format**: `hound` for WAV file operations and a `lame_encoder` module for MP3.
    *   **Async**: `tokio` for asynchronous operations, particularly for WebSocket communication.
    *   **Transcription**: `tokio-tungstenite` for WebSocket communication with the Soniox API.
*   **Frontend**: TypeScript with the Vite build tool. It appears to be using vanilla TypeScript without a major UI framework like React or Vue.
*   **Transcription Service**: Soniox API for real-time speech-to-text.

### Project Structure

*   `src/`: Contains the frontend TypeScript and CSS code.
*   `src-tauri/`: Contains the backend Rust code.
    *   `src-tauri/src/main.rs`: The main entry point for the Rust application.
    *   `src-tauri/src/soniox.rs`: Handles the integration with the Soniox transcription service.
    *   `src-tauri/src/audio.rs`: Manages audio recording and processing.
*   `config/`: Holds configuration files, such as Soniox API credentials.
*   `AGENTS.md` & `CLAUDE.md`: These files provide detailed guidelines and context for AI assistants working on this project.
*   `TODO.md`: Outlines a detailed plan for implementing a pause/resume feature, which is currently not implemented.

### Key Features

*   **Recording Modes**: Supports both manual start/stop recording and Voice Activity Detection (VAD) for automatic recording.
*   **Real-time Transcription**: Displays live transcription of spoken audio using the Soniox API.
*   **Audio Output**: Saves recordings in either WAV or MP3 format.
*   **Audio Visualization**: Provides real-time feedback on audio levels.

### Development Commands

*   **Run frontend only**: `npm run dev`
*   **Run full application**: `npm run tauri dev`
*   **Build frontend**: `npm run build`
*   **Build application for distribution**: `npm run tauri build`
*   **Run backend tests**: `cargo test --manifest-path src-tauri/Cargo.toml`
