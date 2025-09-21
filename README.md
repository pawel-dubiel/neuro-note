# Voice Assistant Recorder

A smart audio recording application that listens to your conversations and provides real-time speech-to-text transcription. Built for the AI-powered voice assistants.

## What it does

This application records audio from your microphone and converts speech to text in real-time and then run selected agent against the transcript.

### Main Features

- **Smart Voice Detection** - Automatically starts recording when you speak and stops when you're quiet
- **Manual Recording** - Click to start/stop recording whenever you want
- **Live Transcription** - See your words appear on screen as you speak (powered by Soniox API)
- **High-Quality Audio** - Save recordings in WAV or MP3 format
- **Real-time Audio Levels** - Visual feedback shows when the microphone picks up sound

### Recording Modes

1. **Voice Detection Mode** - The app listens and automatically records when it hears speech
2. **Manual Mode** - You control when recording starts and stops

## The Big Idea: AI Assistant Integration

This is just the beginning. The goal is to connect this recorder to Large Language Models (LLMs) that can act as intelligent assistants. Here's the vision:

### Future AI Capabilities

- **Smart Interruptions** - The AI assistant can join conversations at the right moment to offer help
- **Context-Aware Suggestions** - Based on what you're discussing, the assistant proposes relevant solutions
- **Voice-Controlled Actions** - Tell the assistant to execute tasks, make requests, or control other systems
- **Agent Coordination** - Multiple AI agents can work together, each with different specialties
- **MCP Protocol Support** - The assistant can connect to various services and tools using the Model Context Protocol

### Example Use Cases

- **Meeting Assistant** - Listens to your business meetings and suggests action items or finds relevant documents
- **Learning Companion** - Helps during study sessions by answering questions or explaining concepts
- **Technical Support** - Monitors technical discussions and offers solutions or documentation
- **Creative Partner** - Assists during brainstorming sessions with ideas and feedback

## Getting Started

### Requirements

- Node.js and npm
- Rust and Cargo
- A Soniox API key for transcription (optional)

### Setup

1. Clone this repository
2. Install dependencies: `npm install`
3. Set up Soniox (optional):
   - Create `config/soniox.local.json`
   - Add your API key: `{"api_key": "your_key_here"}`
4. Run the app: `npm run tauri dev`

### Basic Usage

1. Open the application
2. Choose your recording mode (voice detection or manual)
3. If using Soniox, check "Enable Soniox" and make sure your API key is configured
4. Start talking - your speech will be recorded and transcribed in real-time

## Technical Details

- **Frontend**: TypeScript + Vite for the user interface
- **Backend**: Rust + Tauri for audio processing and system integration
- **Audio**: Cross-platform recording using CPAL library
- **Transcription**: Real-time speech-to-text via Soniox WebSocket API
- **Formats**: Supports WAV and MP3 audio output

## Development

- `npm run dev` - Start development server
- `npm run tauri dev` - Run the full desktop application
- `npm run build` - Build for production

For detailed development information, see `CLAUDE.md` and `AGENTS.md`.


## License

This project is for experimental and educational purposes.

---

*This is an experimental project exploring the future of voice-controlled AI assistants. The recording and transcription features work today, while the AI integration is planned for future development.*
