// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use std::{
    fs::File,
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use tauri::{Emitter, State};

mod utils;
mod audio;
mod soniox;
mod openai;
mod gate;
mod assistants;
mod config;
#[cfg(test)]
mod soniox_test;
pub use audio::AudioWriter;
use crate::utils::log_to_file;
use crate::assistants::{AssistantManager, Assistant};
use crate::config::{ConfigManager, AppConfig};

pub mod lame_encoder;

// State machine for recording operations
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "data")]
enum RecordingState {
    Idle,
    Starting,
    Recording {
        start_time: String, // Serialized timestamp
        elapsed_ms: u64,
    },
    Paused {
        pause_time: String, // Serialized timestamp
        elapsed_ms: u64,
    },
    Resuming,
    Stopping,
}

impl Default for RecordingState {
    fn default() -> Self {
        RecordingState::Idle
    }
}

// Commands for recording operations
#[derive(Debug, Clone)]
enum RecordingCommand {
    Start {
        path: PathBuf,
        format: String,
        quality: String,
    },
    Pause,
    Resume,
    Stop,
}

// Configuration for recording sessions
#[derive(Debug, Clone)]
struct RecordingConfig {
    path: PathBuf,
    format: String,
    quality: String,
}

// Internal state for tracking recording session
#[derive(Debug)]
struct RecordingSession {
    config: RecordingConfig,
    start_time: Instant,
    total_elapsed: Duration,
    state: RecordingState,
}

// AudioWriter moved to crate::audio

// logging moved to crate::utils

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[derive(Clone)]
struct AppState {
    // Enhanced state management with atomic transitions
    current_state: Arc<Mutex<RecordingState>>,
    recording_session: Arc<Mutex<Option<RecordingSession>>>,

    // Command processing
    command_tx: Arc<Mutex<Option<mpsc::Sender<RecordingCommand>>>>,

    // Legacy channels (kept for backward compatibility during transition)
    stop_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    done_rx: Arc<Mutex<Option<mpsc::Receiver<Result<String, String>>>>>,

    // Audio writer state for VAD/auto session
    writer_state: Arc<Mutex<Option<AudioWriter>>>,
    vad_session_path: Arc<Mutex<Option<PathBuf>>>,
    is_writing_enabled: Arc<Mutex<bool>>, // Controls whether samples are written during pause
    // Soniox real-time transcription session (optional)
    soniox_tx: Arc<Mutex<Option<tokio::sync::mpsc::Sender<soniox::AudioChunk>>>>,
    // Track if we're in voice detection mode vs manual recording mode
    is_voice_detection_mode: Arc<Mutex<bool>>,
    // Track if voice is currently being detected (only used in voice detection mode)
    voice_currently_detected: Arc<Mutex<bool>>,
    // Assistant manager for multiple AI assistants
    assistant_manager: Arc<Mutex<AssistantManager>>,
    // App configuration
    app_config: Arc<Mutex<AppConfig>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            current_state: Arc::new(Mutex::new(RecordingState::Idle)),
            recording_session: Arc::new(Mutex::new(None)),
            command_tx: Arc::new(Mutex::new(None)),
            stop_tx: Arc::new(Mutex::new(None)),
            done_rx: Arc::new(Mutex::new(None)),
            writer_state: Arc::new(Mutex::new(None)),
            vad_session_path: Arc::new(Mutex::new(None)),
            is_writing_enabled: Arc::new(Mutex::new(false)),
            soniox_tx: Arc::new(Mutex::new(None)),
            is_voice_detection_mode: Arc::new(Mutex::new(false)),
            voice_currently_detected: Arc::new(Mutex::new(false)),
            assistant_manager: Arc::new(Mutex::new(AssistantManager::empty())),
            app_config: Arc::new(Mutex::new(AppConfig::default())),
        }
    }
}

impl AppState {
    // Atomic state transition with validation
    fn transition_state(&self, from: RecordingState, to: RecordingState, app: &tauri::AppHandle) -> Result<(), String> {
        let mut current_state = self.current_state.lock().map_err(|_| "Failed to lock state")?;

        // Validate transition is allowed
        if *current_state != from {
            return Err(format!("Invalid state transition: expected {:?}, found {:?}", from, *current_state));
        }

        // Update timestamps for state tracking
        let new_state = match &to {
            RecordingState::Recording { .. } => {
                let now = Instant::now();
                RecordingState::Recording {
                    start_time: format!("{:?}", now),
                    elapsed_ms: 0,
                }
            },
            RecordingState::Paused { .. } => {
                if let Some(session) = self.recording_session.lock().unwrap().as_ref() {
                    let elapsed = session.start_time.elapsed() + session.total_elapsed;
                    RecordingState::Paused {
                        pause_time: format!("{:?}", Instant::now()),
                        elapsed_ms: elapsed.as_millis() as u64,
                    }
                } else {
                    return Err("No recording session found for pause".to_string());
                }
            },
            _ => to,
        };

        *current_state = new_state.clone();

        // Emit state change event to UI
        let _ = app.emit("recording-state-changed", &new_state);

        Ok(())
    }

    // Get current state safely
    fn get_current_state(&self) -> Result<RecordingState, String> {
        self.current_state
            .lock()
            .map(|state| state.clone())
            .map_err(|_| "Failed to lock state".to_string())
    }

    // Update writing enabled state atomically
    fn set_writing_enabled(&self, enabled: bool) -> Result<(), String> {
        *self.is_writing_enabled.lock().map_err(|_| "Failed to lock writing state")? = enabled;
        Ok(())
    }

    // Check if writing is enabled
    fn is_writing_enabled(&self) -> bool {
        *self.is_writing_enabled.lock().unwrap_or_else(|e| e.into_inner())
    }
}

// Command processing functions
impl AppState {
    fn process_command(&self, command: RecordingCommand, app: &tauri::AppHandle) -> Result<String, String> {
        match command {
            RecordingCommand::Start { path, format, quality } => {
                self.handle_start_command(path, format, quality, app)
            },
            RecordingCommand::Pause => {
                self.handle_pause_command(app)
            },
            RecordingCommand::Resume => {
                self.handle_resume_command(app)
            },
            RecordingCommand::Stop => {
                self.handle_stop_command(app)
            },
        }
    }

    fn handle_start_command(&self, path: PathBuf, format: String, quality: String, app: &tauri::AppHandle) -> Result<String, String> {
        // Validate we can start
        let current_state = self.get_current_state()?;
        if current_state != RecordingState::Idle {
            return Err(format!("Cannot start recording in state: {:?}", current_state));
        }

        // Transition to Starting state
        self.transition_state(RecordingState::Idle, RecordingState::Starting, app)?;

        // Create recording session
        let config = RecordingConfig { path: path.clone(), format, quality };
        let session = RecordingSession {
            config: config.clone(),
            start_time: Instant::now(),
            total_elapsed: Duration::from_secs(0),
            state: RecordingState::Starting,
        };

        *self.recording_session.lock().map_err(|_| "Failed to lock session")? = Some(session);

        // Enable writing
        self.set_writing_enabled(true)?;

        // Transition to Recording state
        self.transition_state(RecordingState::Starting, RecordingState::Recording {
            start_time: format!("{:?}", Instant::now()),
            elapsed_ms: 0
        }, app)?;

        Ok(format!("Started recording to: {}", path.display()))
    }

    fn handle_pause_command(&self, app: &tauri::AppHandle) -> Result<String, String> {
        let current_state = self.get_current_state()?;

        match current_state {
            RecordingState::Recording { .. } => {
                // Disable writing (audio continues capturing but not saved)
                self.set_writing_enabled(false)?;

                // Transition to Paused state
                self.transition_state(current_state, RecordingState::Paused {
                    pause_time: format!("{:?}", Instant::now()),
                    elapsed_ms: 0
                }, app)?;

                Ok("Recording paused".to_string())
            },
            _ => Err(format!("Cannot pause in state: {:?}", current_state))
        }
    }

    fn handle_resume_command(&self, app: &tauri::AppHandle) -> Result<String, String> {
        let current_state = self.get_current_state()?;

        match current_state {
            RecordingState::Paused { .. } => {
                // Transition to Resuming state
                self.transition_state(current_state, RecordingState::Resuming, app)?;

                // Enable writing
                self.set_writing_enabled(true)?;

                // Update session elapsed time
                if let Some(session) = self.recording_session.lock().unwrap().as_mut() {
                    session.total_elapsed = session.start_time.elapsed();
                    session.start_time = Instant::now(); // Reset start time for new segment
                }

                // Transition to Recording state
                self.transition_state(RecordingState::Resuming, RecordingState::Recording {
                    start_time: format!("{:?}", Instant::now()),
                    elapsed_ms: 0,
                }, app)?;

                Ok("Recording resumed".to_string())
            },
            _ => Err(format!("Cannot resume in state: {:?}", current_state))
        }
    }

    fn handle_stop_command(&self, app: &tauri::AppHandle) -> Result<String, String> {
        let current_state = self.get_current_state()?;

        match current_state {
            RecordingState::Recording { .. } | RecordingState::Paused { .. } => {
                // Transition to Stopping state
                self.transition_state(current_state, RecordingState::Stopping, app)?;

                // Disable writing
                self.set_writing_enabled(false)?;

                // For now, we'll rely on the existing stop mechanism
                // The new state system doesn't fully manage the writer yet
                // so we'll return a placeholder path
                let path = if let Some(session) = self.recording_session.lock().unwrap().as_ref() {
                    session.config.path.display().to_string()
                } else {
                    "recording.wav".to_string()
                };

                // Clear session
                *self.recording_session.lock().unwrap() = None;

                // Transition to Idle state
                self.transition_state(RecordingState::Stopping, RecordingState::Idle, app)?;

                Ok(path)
            },
            _ => Err(format!("Cannot stop in state: {:?}", current_state))
        }
    }
}

#[tauri::command]
fn start_recording(
    app: tauri::AppHandle,
    state: State<AppState>,
    path: Option<String>,
    format: Option<String>,
    quality: Option<String>,
) -> Result<String, String> {
    // Check if we can start using the new state system
    let current_state = state.inner().get_current_state().map_err(|e| format!("State error: {}", e))?;
    if current_state != RecordingState::Idle {
        return Err(format!("Recording already in progress: {:?}", current_state));
    }

    // Mark that we're in manual recording mode (not voice detection)
    *state.inner().is_voice_detection_mode.lock().unwrap() = false;

    // Resolve output path
    let target_format = format.as_deref().unwrap_or("wav");
    let file_extension = match target_format {
        "mp3" => "mp3",
        _ => "wav",
    };

    let out_path = if let Some(p) = path {
        PathBuf::from(p)
    } else {
        let mut base = dirs_next::document_dir().unwrap_or_else(|| std::env::temp_dir());
        let ts = chrono::Local::now()
            .format(&format!("recording-%Y%m%d-%H%M%S.{}", file_extension))
            .to_string();
        base.push(ts);
        base
    };

    // Create control channels and spawn the recording thread.
    let (tx, rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel::<Result<String, String>>();
    let finalize_path = out_path.clone();
    let app_for_thread = app.clone();
    let _target_format = format.unwrap_or_else(|| "wav".into());
    let _target_quality = quality.unwrap_or_else(|| "high".into());
    let state_for_thread = state.inner().clone();

    thread::spawn(move || {
        // Prepare audio device/config inside the thread so we don't need Send.
        let host = cpal::default_host();
        let device = match host.default_input_device() {
            Some(d) => d,
            None => {
                let _ = done_tx.send(Err("No input device available".into()));
                return;
            }
        };
        let config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                let _ = done_tx.send(Err(format!("Failed to get default input config: {e}")));
                return;
            }
        };
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        // Prepare audio writer based on format
        let writer = match _target_format.as_str() {
            "mp3" => {
                // Create MP3 encoder
                let mut encoder = match lame_encoder::Lame::new() {
                    Some(enc) => enc,
                    None => {
                        let _ = done_tx.send(Err("Failed to initialize LAME encoder".into()));
                        return;
                    }
                };

                // Configure encoder
                if let Err(e) = encoder.set_sample_rate(sample_rate) {
                    let _ = done_tx.send(Err(format!("Failed to set sample rate: {:?}", e)));
                    return;
                }
                if let Err(e) = encoder.set_channels(channels as u8) {
                    let _ = done_tx.send(Err(format!("Failed to set channels: {:?}", e)));
                    return;
                }

                // Set quality based on quality parameter
                let (bitrate, quality_level) = match _target_quality.as_str() {
                    "verylow" => (64, 9),
                    "low" => (128, 7),
                    "medium" => (192, 5),
                    "high" => (320, 2),
                    _ => (192, 5), // default
                };

                if let Err(e) = encoder.set_kilobitrate(bitrate) {
                    let _ = done_tx.send(Err(format!("Failed to set bitrate: {:?}", e)));
                    return;
                }
                if let Err(e) = encoder.set_quality(quality_level) {
                    let _ = done_tx.send(Err(format!("Failed to set quality: {:?}", e)));
                    return;
                }
                if let Err(e) = encoder.init_params() {
                    let _ = done_tx.send(Err(format!("Failed to initialize encoder params: {:?}", e)));
                    return;
                }

                let file = match File::create(&finalize_path) {
                    Ok(f) => f,
                    Err(e) => {
                        let _ = done_tx.send(Err(format!("Failed to create MP3 file: {e}")));
                        return;
                    }
                };

                Arc::new(Mutex::new(Some(AudioWriter::Mp3 {
                    encoder,
                    file,
                    buffer: Vec::new(),
                    channels,
                })))
            }
            _ => {
                // Default to WAV
                let spec = hound::WavSpec {
                    channels,
                    sample_rate,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                };
                let wav_writer = match hound::WavWriter::create(&finalize_path, spec) {
                    Ok(w) => w,
                    Err(e) => {
                        let _ = done_tx.send(Err(format!("Failed to create WAV: {e}")));
                        return;
                    }
                };
                Arc::new(Mutex::new(Some(AudioWriter::Wav(wav_writer))))
            }
        };

        // Set up recording session in new state system
        let recording_config = RecordingConfig {
            path: finalize_path.clone(),
            format: _target_format.clone(),
            quality: _target_quality.clone(),
        };
        let session = RecordingSession {
            config: recording_config.clone(),
            start_time: std::time::Instant::now(),
            total_elapsed: Duration::from_secs(0),
            state: RecordingState::Recording {
                start_time: format!("{:?}", std::time::Instant::now()),
                elapsed_ms: 0,
            },
        };
        *state_for_thread.recording_session.lock().unwrap() = Some(session);

        // Enable writing initially
        *state_for_thread.is_writing_enabled.lock().unwrap() = true;

        // Transition to Recording state
        let _ = state_for_thread.transition_state(
            RecordingState::Idle,
            RecordingState::Recording {
                start_time: format!("{:?}", std::time::Instant::now()),
                elapsed_ms: 0,
            },
            &app_for_thread
        );

        // Build stream with proper sample type
        let writer_clone = writer.clone();
        let err_fn = |e| eprintln!("Stream error: {e}");
        let stream_cfg: cpal::StreamConfig = config.clone().into();
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build_stream_f32(&device, &stream_cfg, writer_clone, err_fn, app_for_thread.clone(), Arc::new(state_for_thread.clone())),
            cpal::SampleFormat::I16 => build_stream_i16(&device, &stream_cfg, writer_clone, err_fn, app_for_thread.clone(), Arc::new(state_for_thread.clone())),
            cpal::SampleFormat::U16 => build_stream_u16(&device, &stream_cfg, writer_clone, err_fn, app_for_thread.clone(), Arc::new(state_for_thread.clone())),
            _ => Err("Unsupported sample format".into()),
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                let _ = done_tx.send(Err(e));
                return;
            }
        };

        if let Err(e) = stream.play() {
            let _ = done_tx.send(Err(format!("Failed to start stream: {e}")));
            return;
        }

        // Wait for stop signal.
        let _ = rx.recv();
        // Dropping stream stops callbacks.
        drop(stream);
        // Finalize audio file.
        let mut guard = writer.lock().unwrap();
        let result_path = if let Some(w) = guard.take() {
            match w.finalize() {
                Ok(_) => Ok(finalize_path.to_string_lossy().to_string()),
                Err(e) => Err(format!("Finalize failed: {e}")),
            }
        } else {
            Err("No writer to finalize".into())
        };
        let _ = done_tx.send(result_path);
    });

    // Save control channels in state.
    *state.inner().stop_tx.lock().unwrap() = Some(tx);
    *state.inner().done_rx.lock().unwrap() = Some(done_rx);

    Ok(out_path.to_string_lossy().to_string())
}

#[tauri::command]
fn arm_auto_recording(
    app: tauri::AppHandle,
    state: State<AppState>,
    threshold: Option<f32>,
    min_speech_ms: Option<u32>,
    silence_ms: Option<u32>,
    pre_roll_ms: Option<u32>,
    format: Option<String>,
    quality: Option<String>,
) -> Result<(), String> {
    if state.inner().stop_tx.lock().unwrap().is_some() {
        return Err("Another recording is already active".into());
    }
    let (tx, rx) = mpsc::channel::<()>();
    *state.inner().stop_tx.lock().unwrap() = Some(tx);
    *state.inner().done_rx.lock().unwrap() = None;

    // Mark that we're in voice detection mode
    *state.inner().is_voice_detection_mode.lock().unwrap() = true;

    let app_for_thread = app.clone();
    let threshold = threshold.unwrap_or(0.03);
    let min_speech_ms = min_speech_ms.unwrap_or(300);
    let silence_ms = silence_ms.unwrap_or(800);
    let pre_roll_ms = pre_roll_ms.unwrap_or(250);
    let cooldown_ms_default: u32 = 500; // avoid immediate retriggering
    let target_format = format.unwrap_or_else(|| "wav".to_string());
    let target_quality = quality.unwrap_or_else(|| "high".to_string());

    let state_for_thread = state.inner().clone();
    std::thread::spawn(move || {
        log_to_file("Starting voice detection thread");
        let host = cpal::default_host();
        let device = match host.default_input_device() {
            Some(d) => d,
            None => {
                let _ = app_for_thread.emit("vad-error", "No input device");
                return;
            }
        };
        let config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                let _ = app_for_thread.emit("vad-error", format!("{e}"));
                return;
            }
        };
        let sample_rate = config.sample_rate().0 as usize;
        let channels = config.channels() as usize;
        let pre_roll_capacity = ((pre_roll_ms as usize) * sample_rate / 1000) * channels;
        let mut prebuf: std::collections::VecDeque<i16> = std::collections::VecDeque::with_capacity(pre_roll_capacity + 1);

        // Ensure writer and path exist (create on first arm, reuse on resume)
        {
            let mut path_guard = state_for_thread.vad_session_path.lock().unwrap();
            if path_guard.is_none() {
                let file_extension = match target_format.as_str() { "mp3" => "mp3", _ => "wav" };
                let mut base = dirs_next::document_dir().unwrap_or_else(|| std::env::temp_dir());
                let ts = chrono::Local::now().format(&format!("recording-%Y%m%d-%H%M%S.{}", file_extension)).to_string();
                base.push(ts);
                *path_guard = Some(base);
            }
        }
        {
            let mut writer_guard = state_for_thread.writer_state.lock().unwrap();
            if writer_guard.is_none() {
                let path = state_for_thread.vad_session_path.lock().unwrap().as_ref().cloned().unwrap();
                let writer = match target_format.as_str() {
                    "mp3" => {
                        let mut encoder = match lame_encoder::Lame::new() { Some(enc) => enc, None => { let _ = app_for_thread.emit("vad-error", "Failed to initialize LAME encoder"); return; } };
                        if let Err(e) = encoder.set_sample_rate(sample_rate as u32) { let _ = app_for_thread.emit("vad-error", format!("set sr: {:?}", e)); return; }
                        if let Err(e) = encoder.set_channels(channels as u8) { let _ = app_for_thread.emit("vad-error", format!("set ch: {:?}", e)); return; }
                        let (bitrate, quality_level) = match target_quality.as_str() { "verylow" => (64,9), "low" => (128,7), "medium" => (192,5), "high" => (320,2), _ => (192,5) };
                        if let Err(e) = encoder.set_kilobitrate(bitrate) { let _ = app_for_thread.emit("vad-error", format!("set br: {:?}", e)); return; }
                        if let Err(e) = encoder.set_quality(quality_level) { let _ = app_for_thread.emit("vad-error", format!("set q: {:?}", e)); return; }
                        if let Err(e) = encoder.init_params() { let _ = app_for_thread.emit("vad-error", format!("init params: {:?}", e)); return; }
                        let file = match File::create(&path) { Ok(f) => f, Err(e) => { let _ = app_for_thread.emit("vad-error", format!("create mp3 file: {e}")); return; } };
                        AudioWriter::Mp3 { encoder, file, buffer: Vec::new(), channels: channels as u16 }
                    }
                    _ => {
                        let spec = hound::WavSpec { channels: channels as u16, sample_rate: sample_rate as u32, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
                        let wav_writer = match hound::WavWriter::create(&path, spec) { Ok(w) => w, Err(e) => { let _ = app_for_thread.emit("vad-error", format!("create wav: {e}")); return; } };
                        AudioWriter::Wav(wav_writer)
                    }
                };
                *writer_guard = Some(writer);
            }
        }

        // Reuse shared writer and path from state
        let session_writer: Arc<Mutex<Option<AudioWriter>>> = Arc::clone(&state_for_thread.writer_state);
        let session_path = state_for_thread.vad_session_path.lock().unwrap().as_ref().cloned().unwrap_or_else(|| std::env::temp_dir().join("recording.wav"));
        let session_writer_cb = Arc::clone(&session_writer);

        log_to_file(&format!("Created continuous recording file: {}", session_path.to_string_lossy()));
        let mut smoothed = 0.0f32;
        let mut above_ms = 0u32;
        let mut below_ms = 0u32;
        let mut cooldown_left_ms = 0u32;
        let mut is_recording_voice = false; // Track if we're currently recording voice

        let err_fn = |e| eprintln!("Stream error: {e}");
        let app_levels = app_for_thread.clone();
        let app_events = app_for_thread.clone();

        // Dynamic threshold calibration (first ~1s)
        let mut threshold_eff: f32 = threshold;
        let mut calib_left_ms: u32 = 1000; // calibrate 1s of noise floor
        let mut noise_energy_accum: f64 = 0.0;
        let mut noise_time_accum_ms: f64 = 0.0;
        let mut noise_peak_max: f32 = 0.0;

        let mut process_chunk = move |as_i16: &[i16]| {
            // Compute peak/rms for meter
            let mut peak = 0.0f32;
            let mut sum_sq = 0.0f32;
            for &s in as_i16 {
                let f = (s as f32) / (i16::MAX as f32);
                let af = f.abs();
                peak = if af > peak { af } else { peak };
                sum_sq += f * f;
            }
            let rms = if as_i16.len() > 0 {
                (sum_sq / (as_i16.len() as f32)).sqrt()
            } else {
                0.0
            };
            let _ = app_levels.emit("audio-level", LevelPayload { rms, peak });

            // Update VAD state
            smoothed = 0.9 * smoothed + 0.1 * rms;
            let chunk_ms = (as_i16.len() / channels) as f32 * 1000.0 / (sample_rate as f32);

            // Calibrate noise floor during the first second
            if calib_left_ms > 0 {
                let used_ms = calib_left_ms.min(chunk_ms as u32) as f64;
                noise_energy_accum += (rms as f64) * used_ms;
                noise_time_accum_ms += used_ms;
                calib_left_ms = calib_left_ms.saturating_sub(chunk_ms as u32);
                noise_peak_max = noise_peak_max.max(peak);
                if calib_left_ms == 0 && noise_time_accum_ms > 0.0 {
                    let noise_avg = (noise_energy_accum / noise_time_accum_ms) as f32;
                    let dyn_thr = (noise_avg * 6.0).max(noise_peak_max * 0.6).max(0.01);
                    threshold_eff = threshold_eff.max(dyn_thr);
                    log_to_file(&format!("Threshold calibrated: {:.4} (noise_avg={:.4}, dyn_thr={:.4})", threshold_eff, noise_avg, dyn_thr));
                    let _ = app_events.emit("vad-threshold", format!("{:.4}", threshold_eff));
                }
            }

            // Maintain pre-roll buffer
            for &s in as_i16 {
                if prebuf.len() >= pre_roll_capacity {
                    prebuf.pop_front();
                }
                prebuf.push_back(s);
            }

            // Voice detection with continuous file recording
            if cooldown_left_ms > 0 { cooldown_left_ms = cooldown_left_ms.saturating_sub(chunk_ms as u32); }

            // Zero-crossing rate heuristic to reject constant hum
            let mut zc = 0u32; let step = channels.max(1); let mut prev = 0i16;
            for (i, &s) in as_i16.iter().step_by(step).enumerate() { if i > 0 && ((s ^ prev) < 0) { zc += 1; } prev = s; }
            let zcr = if chunk_ms > 0.0 { (zc as f32) * 1000.0 / chunk_ms } else { 0.0 };
            let zcr_ok = zcr > 50.0;

            // More lenient voice detection - removed strict peak requirement and ZCR
            let voice_detected = cooldown_left_ms == 0 && (smoothed > threshold_eff || peak > threshold_eff * 0.8);

            // Log detection attempts every second for debugging
            static mut DEBUG_COUNTER: u32 = 0;
            unsafe {
                DEBUG_COUNTER += chunk_ms as u32;
                if DEBUG_COUNTER >= 1000 {
                    log_to_file(&format!("Detection check: smoothed={:.4}, peak={:.4}, threshold={:.4}, zcr={:.1}, zcr_ok={}, cooldown={}ms",
                        smoothed, peak, threshold_eff, zcr, zcr_ok, cooldown_left_ms));
                    DEBUG_COUNTER = 0;
                }
            }

            if voice_detected {
                above_ms += chunk_ms as u32;
                if above_ms % 100 == 0 { // Log every 100ms while detecting voice
                    log_to_file(&format!("Voice detected: {}ms (smoothed={:.3}, peak={:.3}, threshold={:.3}, zcr={:.1})", above_ms, smoothed, peak, threshold_eff, zcr));
                }

                // Start recording if we hit the minimum speech threshold and aren't already recording
                if above_ms >= min_speech_ms && !is_recording_voice {
                    is_recording_voice = true;
                    // Update global state for Soniox
                    *state_for_thread.voice_currently_detected.lock().unwrap() = true;
                    log_to_file(&format!("Started recording voice after {}ms of speech", above_ms));
                    let _ = app_events.emit("vad-segment-start", "");

                    // Write pre-roll buffer to the continuous file
                    if let Some(w) = session_writer_cb.lock().unwrap().as_mut() {
                        log_to_file(&format!("Writing pre-roll buffer with {} samples", prebuf.len()));
                        for &s in prebuf.iter() { let _ = w.write_sample(s); }
                    }
                }

                // Write current audio to file if we're recording voice
                if is_recording_voice {
                    if let Some(w) = session_writer_cb.lock().unwrap().as_mut() {
                        for &s in as_i16 { let _ = w.write_sample(s); }
                    }
                }

                below_ms = 0; // Reset silence counter
            } else {
                if above_ms > 0 { // Log when voice detection stops
                    log_to_file(&format!("Voice detection stopped at {}ms (smoothed={:.3}, peak={:.3}, threshold={:.3}, zcr={:.1}, cooldown={}ms)", above_ms, smoothed, peak, threshold_eff, zcr, cooldown_left_ms));
                }
                above_ms = 0;

                // If we're currently recording voice, count silence
                if is_recording_voice {
                    below_ms += chunk_ms as u32;

                    // Continue writing even during silence (to maintain continuity)
                    if let Some(w) = session_writer_cb.lock().unwrap().as_mut() {
                        for &s in as_i16 { let _ = w.write_sample(s); }
                    }

                    // Stop recording after enough silence
                    if below_ms >= silence_ms {
                        is_recording_voice = false;
                        // Update global state for Soniox
                        *state_for_thread.voice_currently_detected.lock().unwrap() = false;
                        below_ms = 0;
                        cooldown_left_ms = cooldown_ms_default;
                        log_to_file(&format!("Stopped recording after {}ms of silence", silence_ms));
                    }
                }
            }

            // Always send audio to Soniox for better context
            // Voice detection filtering will happen at the transcript display level
            if let Ok(lock) = state_for_thread.soniox_tx.lock() {
                if let Some(tx) = lock.as_ref() {
                    let _ = tx.try_send(soniox::AudioChunk {
                        samples: as_i16.to_vec(),
                        channels: channels as u16,
                        sample_rate: sample_rate as u32,
                    });
                }
            }
        };

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let cfg: cpal::StreamConfig = config.clone().into();
                let data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    // convert to i16 interleaved
                    let mut buf: Vec<i16> = Vec::with_capacity(data.len());
                    for &x in data { buf.push((x.clamp(-1.0,1.0) * i16::MAX as f32) as i16); }
                    process_chunk(&buf);
                };
                device.build_input_stream(&cfg, data_fn, err_fn, None)
            }
            cpal::SampleFormat::I16 => {
                let cfg: cpal::StreamConfig = config.clone().into();
                let data_fn = move |data: &[i16], _: &cpal::InputCallbackInfo| { process_chunk(data); };
                device.build_input_stream(&cfg, data_fn, err_fn, None)
            }
            cpal::SampleFormat::U16 => {
                let cfg: cpal::StreamConfig = config.clone().into();
                let data_fn = move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    let mut buf: Vec<i16> = Vec::with_capacity(data.len());
                    for &x in data { buf.push((x as i32 - 32768) as i16); }
                    process_chunk(&buf);
                };
                device.build_input_stream(&cfg, data_fn, err_fn, None)
            }
            _ => Err(cpal::BuildStreamError::StreamConfigNotSupported)
        };
        let Ok(stream) = stream else {
            let _ = app_for_thread.emit("vad-error", "build stream failed");
            return;
        };
        if let Err(e) = stream.play() {
            log_to_file(&format!("Failed to start audio stream: {}", e));
            let _ = app_for_thread.emit("vad-error", format!("{e}"));
            return;
        }
        log_to_file("Voice detection stream started successfully");

        // Wait for disarm
        let _ = rx.recv();
        drop(stream);

        // Do not finalize here; keep file open for pause/resume.
    });

    Ok(())
}

#[tauri::command]
fn disarm_auto_recording(state: State<AppState>) -> Result<(), String> {
    if let Some(tx) = state.inner().stop_tx.lock().unwrap().take() {
        // Reset voice detection state
        *state.inner().is_voice_detection_mode.lock().unwrap() = false;
        *state.inner().voice_currently_detected.lock().unwrap() = false;
        let _ = tx.send(());
        Ok(())
    } else {
        Err("Auto recording not active".into())
    }
}

#[tauri::command]
fn finalize_auto_recording(app: tauri::AppHandle, state: State<AppState>) -> Result<String, String> {
    // Ensure stream is not active
    if state.inner().stop_tx.lock().unwrap().is_some() {
        return Err("Please pause/stop the stream before finalizing".into());
    }
    let path = state
        .inner()
        .vad_session_path
        .lock()
        .unwrap()
        .as_ref()
        .cloned()
        .ok_or_else(|| "No active voice session".to_string())?;

    let res = {
        let mut guard = state.inner().writer_state.lock().unwrap();
        if let Some(w) = guard.take() {
            match w.finalize() {
                Ok(_) => Ok(path.to_string_lossy().to_string()),
                Err(e) => Err(format!("Failed to finalize: {e}")),
            }
        } else {
            Err("No writer to finalize".into())
        }
    };

    match &res {
        Ok(p) => { let _ = app.emit("vad-segment-saved", p.clone()); },
        Err(e) => { let _ = app.emit("vad-error", e.clone()); },
    }

    // Clear session path after finalization attempt
    *state.inner().vad_session_path.lock().unwrap() = None;
    res
}

#[tauri::command]
fn stop_recording(state: State<AppState>) -> Result<String, String> {
    // Take the stop sender and completion receiver from state.
    let tx_opt = state.inner().stop_tx.lock().unwrap().take();
    let done_rx_opt = state.inner().done_rx.lock().unwrap().take();

    let Some(tx) = tx_opt else { return Err("No active recording".into()); };
    let Some(done_rx) = done_rx_opt else { return Err("Internal error: no completion channel".into()); };

    // Signal stop and wait for completion.
    let _ = tx.send(());
    match done_rx.recv() {
        Ok(res) => res,
        Err(e) => Err(format!("Recording thread error: {e}")),
    }
}

// New pause/resume commands using the enhanced state management
#[tauri::command]
fn pause_recording(
    app: tauri::AppHandle,
    state: State<AppState>,
) -> Result<String, String> {
    state.inner().process_command(RecordingCommand::Pause, &app)
}

#[tauri::command]
fn resume_recording(
    app: tauri::AppHandle,
    state: State<AppState>,
) -> Result<String, String> {
    state.inner().process_command(RecordingCommand::Resume, &app)
}

#[tauri::command]
fn get_recording_state(state: State<AppState>) -> Result<RecordingState, String> {
    state.inner().get_current_state()
}

// Soniox Tauri commands
#[tauri::command]
fn start_soniox_session(app: tauri::AppHandle, state: State<AppState>, mut opts: soniox::SonioxOptions) -> Result<(), String> {
    if state.inner().soniox_tx.lock().unwrap().is_some() {
        return Err("Soniox session already running".into());
    }
    if opts.api_key.trim().is_empty() {
        // Try environment variable
        if let Ok(k) = std::env::var("SONIOX_API_KEY") { opts.api_key = k; }
        // Try config file: config/soniox.local.json
        if opts.api_key.trim().is_empty() {
            let mut cfg_path = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            cfg_path.push("config/soniox.local.json");
            if let Ok(txt) = std::fs::read_to_string(&cfg_path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&txt) {
                    if let Some(k) = json.get("api_key").and_then(|v| v.as_str()) {
                        opts.api_key = k.to_string();
                    }
                }
            }
        }
        if opts.api_key.trim().is_empty() {
            return Err("Missing Soniox API key. Provide api_key, SONIOX_API_KEY env, or config/soniox.local.json".into());
        }
    }
    log_to_file(&format!("Starting Soniox session with API key: {}...", &opts.api_key[..8]));
    let handle = tauri::async_runtime::block_on(soniox::start_session(app, opts))?;
    *state.inner().soniox_tx.lock().unwrap() = Some(handle.tx);
    log_to_file("Soniox session started successfully");
    Ok(())
}

#[tauri::command]
fn stop_soniox_session(state: State<AppState>) -> Result<(), String> {
    *state.inner().soniox_tx.lock().unwrap() = None;
    Ok(())
}

#[tauri::command]
async fn analyze_with_openai(transcript: String, api_key: String, model: Option<String>, assistant_id: Option<String>, last_output: Option<String>, state: State<'_, AppState>) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("OpenAI API key is required".to_string());
    }

    let (system_prompt, output_policy) = {
        let manager = state.assistant_manager.lock().unwrap();
        let assistant = if let Some(id) = assistant_id {
            manager.get_assistant(&id).unwrap_or_else(|| manager.get_default_assistant())
        } else {
            manager.get_default_assistant()
        };
        (assistant.system_prompt.clone(), assistant.output_policy.clone())
    };

    let opts = openai::OpenAIOptions {
        api_key,
        model: model.unwrap_or_else(|| "gpt-4.1".to_string()),
        system_prompt,
        output_policy,
    };

    openai::analyze_conversation(opts, transcript, last_output).await
}

#[derive(serde::Serialize)]
struct GateDecision {
    run: bool,
    instruction: Option<String>,
    reason: Option<String>,
    confidence: Option<f32>,
}

#[tauri::command]
async fn should_run_analysis_gate(
    api_key: String,
    model: Option<String>,
    assistant_id: Option<String>,
    current_transcript: String,
    previous_transcript: String,
    last_output: Option<String>,
    state: State<'_, AppState>,
) -> Result<GateDecision, String> {
    if api_key.is_empty() {
        return Err("OpenAI API key is required".to_string());
    }

    let (system_prompt, gate_instructions) = {
        let manager = state.assistant_manager.lock().unwrap();
        let assistant = if let Some(id) = assistant_id {
            manager.get_assistant(&id).unwrap_or_else(|| manager.get_default_assistant())
        } else {
            manager.get_default_assistant()
        };
        (assistant.system_prompt.clone(), assistant.gate_instructions.clone())
    };

    let gate_model = {
        let config = state.app_config.lock().unwrap();
        config.openai.gate_model.clone()
    };

    let opts = gate::GateOptions {
        api_key,
        model: model.unwrap_or(gate_model),
        main_system_prompt: system_prompt,
        gate_instructions,
    };
    let g = gate::should_run_gate(opts, current_transcript, previous_transcript, last_output).await?;
    Ok(GateDecision { run: g.run, instruction: g.instruction, reason: g.reason, confidence: g.confidence })
}
#[tauri::command]
async fn get_openai_models(api_key: String) -> Result<Vec<String>, String> {
    openai::get_available_models(api_key).await
}

#[tauri::command]
async fn load_assistants(state: State<'_, AppState>) -> Result<(), String> {
    let config_path = "../config/assistants.json";

    // Log current working directory for debugging
    match std::env::current_dir() {
        Ok(cwd) => log_to_file(&format!("Current working directory: {:?}", cwd)),
        Err(e) => log_to_file(&format!("Failed to get current directory: {}", e)),
    }

    match AssistantManager::load_from_file(config_path) {
        Ok(manager) => {
            *state.assistant_manager.lock().unwrap() = manager;
            Ok(())
        }
        Err(e) => {
            let error_msg = format!("Failed to load assistants: {}", e);
            log_to_file(&error_msg);
            Err(error_msg)
        }
    }
}

#[tauri::command]
async fn get_assistants(state: State<'_, AppState>) -> Result<Vec<Assistant>, String> {
    let manager = state.assistant_manager.lock().unwrap();
    Ok(manager.list_assistants().into_iter().cloned().collect())
}

#[tauri::command]
async fn get_default_assistant_id(state: State<'_, AppState>) -> Result<String, String> {
    let manager = state.assistant_manager.lock().unwrap();
    Ok(manager.get_default_id().to_string())
}

#[tauri::command]
async fn load_app_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    match ConfigManager::load_config() {
        Ok(config) => {
            *state.app_config.lock().unwrap() = config.clone();
            Ok(config)
        }
        Err(e) => {
            log_to_file(&format!("Failed to load app config: {}", e));
            Err(e)
        }
    }
}

#[tauri::command]
async fn save_app_config(state: State<'_, AppState>, config: AppConfig) -> Result<(), String> {
    match ConfigManager::save_config(&config) {
        Ok(()) => {
            *state.app_config.lock().unwrap() = config;
            Ok(())
        }
        Err(e) => {
            log_to_file(&format!("Failed to save app config: {}", e));
            Err(e)
        }
    }
}

#[tauri::command]
async fn get_app_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    let config = state.app_config.lock().unwrap();
    Ok(config.clone())
}

#[tauri::command]
async fn create_default_config() -> Result<(), String> {
    ConfigManager::create_default_config()
}

#[derive(Serialize, Clone)]
struct LevelPayload {
    rms: f32,
    peak: f32,
}

fn build_stream_f32(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    writer: Arc<Mutex<Option<AudioWriter>>>,
    err_fn: impl Fn(cpal::StreamError) + Send + 'static,
    app: tauri::AppHandle,
    app_state: Arc<AppState>,
) -> Result<cpal::Stream, String> {
    let channels = config.channels;
    let sample_rate = config.sample_rate.0;
    let data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
        let mut peak = 0.0f32;
        let mut sum_sq = 0.0f32;

        // Always calculate audio levels for UI feedback (even when paused)
        for &sample in data {
            let s = sample.clamp(-1.0, 1.0);
            peak = peak.max(s.abs());
            sum_sq += s * s;
        }
        let rms = (sum_sq / (data.len().max(1) as f32)).sqrt();
        let _ = app.emit("audio-level", LevelPayload { rms, peak });

        // Only write samples if recording is not paused
        if app_state.is_writing_enabled() {
            if let Ok(mut guard) = writer.lock() {
                if let Some(w) = guard.as_mut() {
                    for &sample in data {
                        let s = sample.clamp(-1.0, 1.0);
                        let i = (s * i16::MAX as f32) as i16;
                        // Proper error handling instead of ignoring failures
                        if let Err(e) = w.write_sample(i) {
                            eprintln!("Warning: Failed to write audio sample: {}", e);
                            // Could emit an error event to UI here if needed
                        }
                    }
                }
            }
        }

        // Always send audio to Soniox for better context
        // Voice detection filtering will happen at the transcript display level
        if let Ok(lock) = app_state.soniox_tx.lock() {
            if let Some(tx) = lock.as_ref() {
                let _ = tx.try_send(soniox::AudioChunk {
                    samples: data.iter().map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).collect(),
                    channels,
                    sample_rate,
                });
            }
        }
    };

    device
        .build_input_stream(config, data_fn, err_fn, None)
        .map_err(|e| format!("Build stream failed: {e}"))
}

fn build_stream_i16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    writer: Arc<Mutex<Option<AudioWriter>>>,
    err_fn: impl Fn(cpal::StreamError) + Send + 'static,
    app: tauri::AppHandle,
    app_state: Arc<AppState>,
) -> Result<cpal::Stream, String> {
    let channels = config.channels;
    let sample_rate = config.sample_rate.0;
    let data_fn = move |data: &[i16], _: &cpal::InputCallbackInfo| {
        let mut peak = 0.0f32;
        let mut sum_sq = 0.0f32;

        // Always calculate audio levels for UI feedback (even when paused)
        for &sample in data {
            let s = (sample as f32) / (i16::MAX as f32);
            peak = peak.max(s.abs());
            sum_sq += s * s;
        }
        let rms = (sum_sq / (data.len().max(1) as f32)).sqrt();
        let _ = app.emit("audio-level", LevelPayload { rms, peak });

        // Only write samples if recording is not paused
        if app_state.is_writing_enabled() {
            if let Ok(mut guard) = writer.lock() {
                if let Some(w) = guard.as_mut() {
                    for &sample in data {
                        if let Err(e) = w.write_sample(sample) {
                            eprintln!("Warning: Failed to write audio sample: {}", e);
                        }
                    }
                }
            }
        }

        // Always send audio to Soniox for better context
        // Voice detection filtering will happen at the transcript display level
        if let Ok(lock) = app_state.soniox_tx.lock() {
            if let Some(tx) = lock.as_ref() {
                let _ = tx.try_send(soniox::AudioChunk {
                    samples: data.to_vec(),
                    channels,
                    sample_rate,
                });
            }
        }
    };

    device
        .build_input_stream(config, data_fn, err_fn, None)
        .map_err(|e| format!("Build stream failed: {e}"))
}

fn build_stream_u16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    writer: Arc<Mutex<Option<AudioWriter>>>,
    err_fn: impl Fn(cpal::StreamError) + Send + 'static,
    app: tauri::AppHandle,
    app_state: Arc<AppState>,
) -> Result<cpal::Stream, String> {
    let channels = config.channels;
    let sample_rate = config.sample_rate.0;
    let data_fn = move |data: &[u16], _: &cpal::InputCallbackInfo| {
        let mut peak = 0.0f32;
        let mut sum_sq = 0.0f32;

        // Always calculate audio levels for UI feedback (even when paused)
        for &sample in data {
            let s = ((sample as i32 - 32768) as f32) / (i16::MAX as f32);
            peak = peak.max(s.abs());
            sum_sq += s * s;
        }
        let rms = (sum_sq / (data.len().max(1) as f32)).sqrt();
        let _ = app.emit("audio-level", LevelPayload { rms, peak });

        // Only write samples if recording is not paused
        if app_state.is_writing_enabled() {
            if let Ok(mut guard) = writer.lock() {
                if let Some(w) = guard.as_mut() {
                    for &sample in data {
                        let i = (sample as i32 - 32768) as i16;
                        if let Err(e) = w.write_sample(i) {
                            eprintln!("Warning: Failed to write audio sample: {}", e);
                        }
                    }
                }
            }
        }

        // Always send audio to Soniox for better context
        // Voice detection filtering will happen at the transcript display level
        if let Ok(lock) = app_state.soniox_tx.lock() {
            if let Some(tx) = lock.as_ref() {
                let _ = tx.try_send(soniox::AudioChunk {
                    samples: data.iter().map(|&u| (u as i32 - 32768) as i16).collect(),
                    channels,
                    sample_rate,
                });
            }
        }
    };

    device
        .build_input_stream(config, data_fn, err_fn, None)
        .map_err(|e| format!("Build stream failed: {e}"))
}


#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            greet,
            start_recording,
            stop_recording,
            arm_auto_recording,
            disarm_auto_recording,
            finalize_auto_recording,
            pause_recording,
            resume_recording,
            get_recording_state,
            start_soniox_session,
            stop_soniox_session,
            analyze_with_openai,
            get_openai_models,
            should_run_analysis_gate,
            load_assistants,
            get_assistants,
            get_default_assistant_id,
            load_app_config,
            save_app_config,
            get_app_config,
            create_default_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
