// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use tauri::{Emitter, State};

pub mod lame_encoder;

pub enum AudioWriter {
    Wav(hound::WavWriter<BufWriter<File>>),
    Mp3 {
        encoder: lame_encoder::Lame,
        file: File,
        buffer: Vec<i16>,
        channels: u16,
    },
}

impl AudioWriter {
    pub fn write_sample(&mut self, sample: i16) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            AudioWriter::Wav(writer) => {
                writer.write_sample(sample)?;
            }
            AudioWriter::Mp3 { buffer, channels, .. } => {
                buffer.push(sample);

                // Process in chunks when we have enough samples
                let samples_per_frame = 1152 * (*channels as usize);
                if buffer.len() >= samples_per_frame {
                    self.flush_mp3_buffer()?;
                }
            }
        }
        Ok(())
    }

    fn flush_mp3_buffer(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let AudioWriter::Mp3 { encoder, file, buffer, channels } = self {
            if buffer.is_empty() {
                return Ok(());
            }

            let samples_per_channel = buffer.len() / (*channels as usize);
            let mut left_channel = Vec::with_capacity(samples_per_channel);
            let mut right_channel = Vec::with_capacity(samples_per_channel);

            if *channels == 1 {
                // Mono: duplicate to both channels
                left_channel = buffer.clone();
                right_channel = buffer.clone();
            } else {
                // Stereo: interleaved
                for chunk in buffer.chunks(2) {
                    if chunk.len() >= 2 {
                        left_channel.push(chunk[0]);
                        right_channel.push(chunk[1]);
                    } else if chunk.len() == 1 {
                        left_channel.push(chunk[0]);
                        right_channel.push(0);
                    }
                }
            }

            // Encode to MP3
            let mut mp3_buffer = vec![0u8; (left_channel.len() * 5 / 4 + 7200).max(16384)];
            match encoder.encode(&left_channel, &right_channel, &mut mp3_buffer) {
                Ok(bytes_written) => {
                    if bytes_written > 0 {
                        file.write_all(&mp3_buffer[..bytes_written])?;
                        log_to_file(&format!("MP3: Encoded {} samples -> {} bytes", buffer.len(), bytes_written));
                    }
                }
                Err(e) => {
                    log_to_file(&format!("MP3 encode error: {:?}", e));
                }
            }

            buffer.clear();
        }
        Ok(())
    }

    pub fn finalize(mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Flush any remaining samples first
        self.flush_mp3_buffer()?;

        match self {
            AudioWriter::Wav(writer) => {
                writer.finalize()?;
            }
            AudioWriter::Mp3 { mut encoder, mut file, .. } => {
                // Flush any remaining encoded data from LAME
                let mut final_buffer = vec![0u8; 7200]; // LAME documentation suggests 7200 bytes max for flush
                match encoder.flush(&mut final_buffer) {
                    Ok(bytes_written) => {
                        if bytes_written > 0 {
                            file.write_all(&final_buffer[..bytes_written])?;
                        }
                    }
                    Err(e) => {
                        eprintln!("LAME flush error: {:?}", e);
                    }
                }
                file.flush()?;
            }
        }
        Ok(())
    }
}

fn log_to_file(message: &str) {
    if let Some(mut docs_dir) = dirs_next::document_dir() {
        docs_dir.push("vad_debug.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&docs_dir)
        {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let _ = writeln!(file, "[{}] {}", timestamp, message);
            let _ = file.flush();
        }
    }
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[derive(Default)]
struct AppState {
    // Control channel to stop the recording thread.
    stop_tx: Mutex<Option<mpsc::Sender<()>>>,
    // Receives completion result (saved path or error) from the recording thread.
    done_rx: Mutex<Option<mpsc::Receiver<Result<String, String>>>>,
}

#[tauri::command]
fn start_recording(
    app: tauri::AppHandle,
    state: State<AppState>,
    path: Option<String>,
    format: Option<String>,
    quality: Option<String>,
) -> Result<String, String> {
    // Prevent double-start
    if state.inner().stop_tx.lock().unwrap().is_some() {
        return Err("Recording already in progress".into());
    }

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

        // Build stream with proper sample type
        let writer_clone = writer.clone();
        let err_fn = |e| eprintln!("Stream error: {e}");
        let stream_cfg: cpal::StreamConfig = config.clone().into();
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build_stream_f32(&device, &stream_cfg, writer_clone, err_fn, app_for_thread.clone()),
            cpal::SampleFormat::I16 => build_stream_i16(&device, &stream_cfg, writer_clone, err_fn, app_for_thread.clone()),
            cpal::SampleFormat::U16 => build_stream_u16(&device, &stream_cfg, writer_clone, err_fn, app_for_thread.clone()),
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

    let app_for_thread = app.clone();
    let threshold = threshold.unwrap_or(0.03);
    let min_speech_ms = min_speech_ms.unwrap_or(300);
    let silence_ms = silence_ms.unwrap_or(800);
    let pre_roll_ms = pre_roll_ms.unwrap_or(250);
    let cooldown_ms_default: u32 = 500; // avoid immediate retriggering
    let target_format = format.unwrap_or_else(|| "wav".to_string());
    let target_quality = quality.unwrap_or_else(|| "high".to_string());

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

        // Create one continuous file for the entire session
        let file_extension = match target_format.as_str() {
            "mp3" => "mp3",
            _ => "wav",
        };

        let mut base = dirs_next::document_dir().unwrap_or_else(|| std::env::temp_dir());
        let ts = chrono::Local::now().format(&format!("recording-%Y%m%d-%H%M%S.{}", file_extension)).to_string();
        base.push(ts);

        // Create audio writer based on format
        let session_writer = match target_format.as_str() {
            "mp3" => {
                // Create MP3 encoder
                let mut encoder = match lame_encoder::Lame::new() {
                    Some(enc) => enc,
                    None => {
                        let _ = app_for_thread.emit("vad-error", "Failed to initialize LAME encoder");
                        return;
                    }
                };

                // Configure encoder
                if let Err(e) = encoder.set_sample_rate(sample_rate as u32) {
                    let _ = app_for_thread.emit("vad-error", format!("Failed to set sample rate: {:?}", e));
                    return;
                }
                if let Err(e) = encoder.set_channels(channels as u8) {
                    let _ = app_for_thread.emit("vad-error", format!("Failed to set channels: {:?}", e));
                    return;
                }

                // Set quality based on quality parameter
                let (bitrate, quality_level) = match target_quality.as_str() {
                    "verylow" => (64, 9),
                    "low" => (128, 7),
                    "medium" => (192, 5),
                    "high" => (320, 2),
                    _ => (192, 5), // default
                };

                if let Err(e) = encoder.set_kilobitrate(bitrate) {
                    let _ = app_for_thread.emit("vad-error", format!("Failed to set bitrate: {:?}", e));
                    return;
                }
                if let Err(e) = encoder.set_quality(quality_level) {
                    let _ = app_for_thread.emit("vad-error", format!("Failed to set quality: {:?}", e));
                    return;
                }
                if let Err(e) = encoder.init_params() {
                    let _ = app_for_thread.emit("vad-error", format!("Failed to initialize encoder params: {:?}", e));
                    return;
                }

                let file = match File::create(&base) {
                    Ok(f) => f,
                    Err(e) => {
                        let _ = app_for_thread.emit("vad-error", format!("Failed to create MP3 file: {e}"));
                        return;
                    }
                };

                log_to_file(&format!("Created MP3 encoder with bitrate={}, quality_level={}, channels={}", bitrate, quality_level, channels));
                Arc::new(Mutex::new(Some(AudioWriter::Mp3 {
                    encoder,
                    file,
                    buffer: Vec::new(),
                    channels: channels as u16,
                })))
            }
            _ => {
                // Default to WAV
                let spec = hound::WavSpec {
                    channels: channels as u16,
                    sample_rate: sample_rate as u32,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                };
                let wav_writer = match hound::WavWriter::create(&base, spec) {
                    Ok(w) => w,
                    Err(e) => {
                        log_to_file(&format!("Failed to create session WAV file: {}", e));
                        let _ = app_for_thread.emit("vad-error", format!("create session wav: {e}"));
                        return;
                    }
                };
                Arc::new(Mutex::new(Some(AudioWriter::Wav(wav_writer))))
            }
        };
        let session_path = base.clone();
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
                        below_ms = 0;
                        cooldown_left_ms = cooldown_ms_default;
                        log_to_file(&format!("Stopped recording after {}ms of silence", silence_ms));
                    }
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

        // Finalize the continuous session file
        {
            if let Some(w) = session_writer.lock().unwrap().take() {
                log_to_file(&format!("Finalizing continuous recording session: {}", session_path.to_string_lossy()));
                match w.finalize() {
                    Ok(_) => {
                        log_to_file("Successfully finalized continuous recording session");
                        let _ = app_for_thread.emit("vad-segment-saved", session_path.to_string_lossy().to_string());
                    }
                    Err(e) => {
                        log_to_file(&format!("Error finalizing continuous recording session: {:?}", e));
                        let _ = app_for_thread.emit("vad-error", format!("Failed to finalize recording: {:?}", e));
                    }
                }
            } else {
                log_to_file("No writer to finalize in continuous recording session");
            };
        }
    });

    Ok(())
}

#[tauri::command]
fn disarm_auto_recording(state: State<AppState>) -> Result<(), String> {
    if let Some(tx) = state.inner().stop_tx.lock().unwrap().take() {
        let _ = tx.send(());
        Ok(())
    } else {
        Err("Auto recording not active".into())
    }
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
) -> Result<cpal::Stream, String> {
    let data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
        if let Ok(mut guard) = writer.lock() {
            if let Some(w) = guard.as_mut() {
                let mut peak = 0.0f32;
                let mut sum_sq = 0.0f32;
                for &sample in data {
                    let s = sample.clamp(-1.0, 1.0);
                    peak = peak.max(s.abs());
                    sum_sq += s * s;
                    let i = (s * i16::MAX as f32) as i16;
                    let _ = w.write_sample(i);
                }
                let rms = (sum_sq / (data.len().max(1) as f32)).sqrt();
                let _ = app.emit("audio-level", LevelPayload { rms, peak });
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
) -> Result<cpal::Stream, String> {
    let data_fn = move |data: &[i16], _: &cpal::InputCallbackInfo| {
        if let Ok(mut guard) = writer.lock() {
            if let Some(w) = guard.as_mut() {
                let mut peak = 0.0f32;
                let mut sum_sq = 0.0f32;
                for &sample in data {
                    let s = (sample as f32) / (i16::MAX as f32);
                    peak = peak.max(s.abs());
                    sum_sq += s * s;
                    let _ = w.write_sample(sample);
                }
                let rms = (sum_sq / (data.len().max(1) as f32)).sqrt();
                let _ = app.emit("audio-level", LevelPayload { rms, peak });
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
) -> Result<cpal::Stream, String> {
    let data_fn = move |data: &[u16], _: &cpal::InputCallbackInfo| {
        if let Ok(mut guard) = writer.lock() {
            if let Some(w) = guard.as_mut() {
                let mut peak = 0.0f32;
                let mut sum_sq = 0.0f32;
                for &sample in data {
                    let s = ((sample as i32 - 32768) as f32) / (i16::MAX as f32);
                    peak = peak.max(s.abs());
                    sum_sq += s * s;
                    let i = (sample as i32 - 32768) as i16;
                    let _ = w.write_sample(i);
                }
                let rms = (sum_sq / (data.len().max(1) as f32)).sqrt();
                let _ = app.emit("audio-level", LevelPayload { rms, peak });
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
        .invoke_handler(tauri::generate_handler![greet, start_recording, stop_recording, arm_auto_recording, disarm_auto_recording])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}