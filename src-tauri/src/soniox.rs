use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::Emitter;
use tokio::{select, sync::mpsc};
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite::protocol::Message};

use crate::utils::log_to_file;

const SONIOX_URL: &str = "wss://stt-rt.soniox.com/transcribe-websocket";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SonioxOptions {
    #[serde(alias = "apiKey")]
    pub api_key: String,
    #[serde(default = "default_audio_format", alias = "audioFormat")]
    pub audio_format: String, // "auto" or "pcm_s16le"
    #[serde(default = "default_translation")]
    pub translation: String, // "none" | "one_way" | "two_way"
}

fn default_audio_format() -> String {
    "pcm_s16le".into()
}
fn default_translation() -> String {
    "none".into()
}

#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub samples: Vec<i16>,
    pub channels: u16,
    pub sample_rate: u32,
}

#[derive(Debug, Clone)]
pub enum SonioxControl {
    ClearTranscript,
}

#[derive(Clone)]
pub struct SonioxHandle {
    pub tx: mpsc::Sender<AudioChunk>,
    pub ctrl: mpsc::Sender<SonioxControl>,
}

impl SonioxHandle {
    pub fn try_send(&self, chunk: AudioChunk) {
        let _ = self.tx.try_send(chunk);
    }
}

pub async fn start_session(
    app: tauri::AppHandle,
    opts: SonioxOptions,
) -> Result<SonioxHandle, String> {
    // Channel from audio thread to WS task
    let (tx, mut rx) = mpsc::channel::<AudioChunk>(32);
    let (ctrl_tx, mut ctrl_rx) = mpsc::channel::<SonioxControl>(8);

    let app_for_task = app.clone();
    let opts_for_task = opts.clone();

    tauri::async_runtime::spawn(async move {
        log_to_file("Soniox: connecting...");
        let _ = app_for_task.emit("soniox-status", "connecting");

        let (mut ws, _resp) =
            match connect_async_tls_with_config(SONIOX_URL, None, false, None).await {
                Ok(pair) => pair,
                Err(e) => {
                    let _ = app_for_task.emit("soniox-error", format!("connect failed: {e}"));
                    return;
                }
            };
        let _ = app_for_task.emit("soniox-status", "connected");

        let mut config = json!({
          "api_key": opts_for_task.api_key,
          "model": "stt-rt-preview-v2",
          "language_hints": ["en", "pl"],
          "enable_language_identification": true,
          "enable_speaker_diarization": true,
          "context": "",
          "enable_endpoint_detection": true,
        });

        if opts_for_task.audio_format == "auto" {
            config["audio_format"] = json!("auto");
        } else {
            config["audio_format"] = json!("pcm_s16le");
            config["sample_rate"] = json!(16000);
            config["num_channels"] = json!(1);
        }

        match opts_for_task.translation.as_str() {
            "one_way" => {
                config["translation"] = json!({"type": "one_way", "target_language": "es"});
            }
            "two_way" => {
                config["translation"] =
                    json!({"type": "two_way", "language_a": "en", "language_b": "es"});
            }
            _ => {}
        }

        if let Err(e) = ws.send(Message::Text(config.to_string())).await {
            let _ = app_for_task.emit("soniox-error", format!("send config failed: {e}"));
            return;
        }
        let _ = app_for_task.emit("soniox-status", "config_sent");

        // Buffers for rendering tokens and tracking currently emitted text
        let mut final_tokens: Vec<serde_json::Value> = Vec::new();
        let mut last_emitted_text: String = String::new();
        let mut suppress_repeat: Option<String> = None;

        // Reader task: receive messages and render transcript
        let (mut ws_sink, mut ws_reader) = ws.split();

        // Pump loop: read from both audio channel and ws
        let mut sent_bytes: usize = 0;
        loop {
            select! {
              cmd = ctrl_rx.recv() => {
                if let Some(control) = cmd {
                    match control {
                        SonioxControl::ClearTranscript => {
                            log_to_file("Soniox: clearing transcript tokens via control command");
                            if suppress_repeat.is_none() {
                                suppress_repeat = Some(last_emitted_text.clone());
                            }
                            final_tokens.clear();
                            last_emitted_text.clear();
                        }
                    }
                }
              }
              // Audio: convert and send binary frames
              maybe_chunk = rx.recv() => {
                if let Some(chunk) = maybe_chunk {
                  let frame = to_pcm16_mono_16k(&chunk.samples, chunk.channels, chunk.sample_rate);
                  let sz = frame.len();
                  if let Err(e) = ws_sink.send(Message::Binary(frame)).await {
                    let _ = app_for_task.emit("soniox-error", format!("send audio failed: {e}"));
                    break;
                  }
                  sent_bytes += sz;
                  if sent_bytes >= 48000 {
                    let _ = app_for_task.emit("soniox-bytes", sent_bytes);
                    sent_bytes = 0;
                  }
                } else {
                  // Channel closed: send empty frame to end and break
                  let _ = ws_sink.send(Message::Text(String::new())).await; // empty string signals end-of-audio
                  break;
                }
              }
              // Websocket messages
              msg = ws_reader.next() => {
                match msg {
                  Some(Ok(Message::Text(txt))) => {
                    if let Ok(res) = serde_json::from_str::<serde_json::Value>(&txt) {
                      if res.get("error_code").is_some() {
                        let code = res["error_code"].to_string();
                        let msg = res["error_message"].to_string();
                        let _ = app_for_task.emit("soniox-error", format!("{code} - {msg}"));
                        break;
                      }

                      // Debug: Log the raw response
                      log_to_file(&format!("Soniox response: {}", txt));

                      // Collect tokens
                      let mut non_final: Vec<serde_json::Value> = Vec::new();
                      let mut has_tokens = false;
                      if let Some(tokens) = res.get("tokens").and_then(|t| t.as_array()) {
                        for token in tokens {
                          if token.get("text").and_then(|t| t.as_str()).unwrap_or("").is_empty() { continue; }
                          has_tokens = true;
                          if token.get("is_final").and_then(|f| f.as_bool()).unwrap_or(false) {
                            final_tokens.push(token.clone());
                          } else {
                            non_final.push(token.clone());
                          }
                        }
                      }

                      // Always emit transcript updates to show real-time progress
                      let text = render_tokens(&final_tokens, &non_final);

                      // If we have any meaningful content, emit it
                      if has_tokens || !text.is_empty() {
                        if suppress_repeat.is_some() {
                          if suppress_repeat.as_deref() == Some(text.as_str()) {
                            // Skip emitting the same text immediately after a clear request
                            continue;
                          } else {
                            suppress_repeat = None;
                          }
                        }
                        log_to_file(&format!("Emitting transcript: '{}'", text));
                        last_emitted_text = text.clone();
                        let _ = app_for_task.emit("soniox-transcript", text);
                      } else {
                        // Debug: Even emit empty responses to see if events are working
                        log_to_file("Received Soniox response with no tokens");
                        let _ = app_for_task.emit("soniox-transcript", "[no speech detected]");
                        last_emitted_text = "[no speech detected]".to_string();
                      }

                      if res.get("finished").and_then(|f| f.as_bool()).unwrap_or(false) { let _ = app_for_task.emit("soniox-status", "finished"); break; }
                    }
                  }
                  Some(Ok(Message::Binary(_bin))) => {
                    // ignore binary messages from server
                  }
                  Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) | Some(Ok(Message::Frame(_))) => {
                    // ignore control frames
                  }
                  Some(Ok(Message::Close(_))) => { let _ = app_for_task.emit("soniox-status", "closed"); break; },
                  Some(Err(e)) => { let _ = app_for_task.emit("soniox-error", format!("ws read error: {e}")); break; }
                  None => break,
                }
              }
            }
        }

        log_to_file("Soniox: session ended");
        let _ = app_for_task.emit("soniox-status", "ended");
    });

    Ok(SonioxHandle { tx, ctrl: ctrl_tx })
}

pub fn render_tokens(
    final_tokens: &Vec<serde_json::Value>,
    non_final_tokens: &Vec<serde_json::Value>,
) -> String {
    use std::collections::HashMap;

    let mut speaker_order: Vec<i64> = Vec::new();
    let mut speaker_text: HashMap<i64, String> = HashMap::new();
    let mut speaker_lang: HashMap<i64, String> = HashMap::new();
    let mut last_speaker_seen: Option<i64> = None;

    for token in final_tokens {
        let raw = token
            .get("text")
            .and_then(|t| t.as_str())
            .map(clean_transcript_text_preserve_spacing)
            .unwrap_or_default();
        if raw.trim().is_empty() {
            continue;
        }

        let speaker = extract_speaker_id(token.get("speaker"), None);
        let language = token
            .get("language")
            .and_then(|l| l.as_str())
            .unwrap_or("?")
            .to_string();

        speaker_lang.entry(speaker).or_insert(language);
        speaker_text
            .entry(speaker)
            .or_default()
            .push_str(&raw);

        if !speaker_order.contains(&speaker) {
            speaker_order.push(speaker);
        }

        last_speaker_seen = Some(speaker);
    }

    for token in non_final_tokens {
        if let Some(text) = token.get("text").and_then(|t| t.as_str()) {
            let clean = clean_transcript_text_preserve_spacing(text);
            if clean.trim().is_empty() {
                continue;
            }

            let speaker = extract_speaker_id(token.get("speaker"), last_speaker_seen);

            speaker_text
                .entry(speaker)
                .or_default()
                .push_str(&clean);

            if !speaker_order.contains(&speaker) {
                speaker_order.push(speaker);
            }
        }
    }

    let mut result = String::new();

    for speaker in &speaker_order {
        if let Some(text) = speaker_text.get(speaker) {
            let formatted = format_transcript_line(text);
            if formatted.is_empty() {
                continue;
            }

            if !result.is_empty() {
                result.push('\n');
            }

            let label = if *speaker > 0 {
                format!("Speaker {}:", speaker)
            } else {
                "Speaker:".to_string()
            };
            let language = speaker_lang.get(speaker).map(|s| s.as_str()).unwrap_or("?");

            result.push_str(&format!("{} [{}] {}", label, language, formatted));
        }
    }

    if !result.is_empty() {
        result.push('\n');
        result.push_str("===============================");
    }

    result
}

fn extract_speaker_id(raw: Option<&serde_json::Value>, fallback: Option<i64>) -> i64 {
    if let Some(value) = raw {
        if let Some(id) = value.as_i64() {
            return id;
        }
        if let Some(text) = value.as_str() {
            if let Ok(parsed) = text.trim().parse::<i64>() {
                return parsed;
            }
        }
    }
    fallback.unwrap_or(0)
}

// Clean up transcript text by removing technical tags but preserving original spacing
fn clean_transcript_text_preserve_spacing(text: &str) -> String {
    let mut cleaned = text.to_string();

    // Remove common technical tags and markers
    cleaned = cleaned.replace("<END>", "");
    cleaned = cleaned.replace("<UNK>", "");
    cleaned = cleaned.replace("<SIL>", "");
    cleaned = cleaned.replace("<NOISE>", "");
    cleaned = cleaned.replace("</s>", "");
    cleaned = cleaned.replace("<s>", "");
    cleaned = cleaned.replace("[NOISE]", "");
    cleaned = cleaned.replace("[SILENCE]", "");
    cleaned = cleaned.replace("[UNKNOWN]", "");

    // Remove any remaining XML-style tags
    let tag_regex =
        regex::Regex::new(r"<[^>]*>").unwrap_or_else(|_| regex::Regex::new("").unwrap());
    cleaned = tag_regex.replace_all(&cleaned, "").to_string();

    // Remove square bracket tags
    let bracket_regex =
        regex::Regex::new(r"\[[^\]]*\]").unwrap_or_else(|_| regex::Regex::new("").unwrap());
    cleaned = bracket_regex.replace_all(&cleaned, "").to_string();

    cleaned
}

// Format a transcript line with proper capitalization and punctuation
fn format_transcript_line(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut formatted = trimmed.to_string();

    // Capitalize first letter if it's not already - safe Unicode handling
    if let Some(first_char) = formatted.chars().next() {
        if first_char.is_lowercase() {
            let mut chars: Vec<char> = formatted.chars().collect();
            if !chars.is_empty() {
                chars[0] = first_char.to_uppercase().next().unwrap_or(first_char);
                formatted = chars.into_iter().collect();
            }
        }
    }

    // Add period at the end if there's no punctuation
    let last_char = formatted.chars().last().unwrap_or(' ');
    if !matches!(last_char, '.' | '!' | '?' | ':' | ';' | ',') {
        formatted.push('.');
    }

    formatted
}

pub fn to_pcm16_mono_16k(samples: &[i16], channels: u16, sample_rate: u32) -> Vec<u8> {
    // Downmix to mono
    let mono: Vec<i16> = if channels <= 1 {
        samples.to_vec()
    } else {
        let mut out = Vec::with_capacity(samples.len() / (channels as usize));
        for chunk in samples.chunks(channels as usize) {
            let sum: i32 = chunk.iter().map(|&s| s as i32).sum();
            let avg = (sum / channels as i32) as i16;
            out.push(avg);
        }
        out
    };

    // Naive resample to 16000 Hz
    if sample_rate == 16_000 {
        // Convert to bytes little-endian
        let mut bytes = Vec::with_capacity(mono.len() * 2);
        for s in mono {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        return bytes;
    }

    let in_rate = sample_rate as f32;
    let out_rate = 16_000.0f32;
    let ratio = out_rate / in_rate;
    let out_len = ((mono.len() as f32) * ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = (i as f32) / ratio;
        let idx = src_pos.floor() as usize;
        let frac = src_pos - (idx as f32);
        let s0 = *mono.get(idx).unwrap_or(&0) as f32;
        let s1 = *mono
            .get((idx + 1).min(mono.len().saturating_sub(1)))
            .unwrap_or(&0) as f32;
        let interp = s0 + (s1 - s0) * frac;
        out.push(interp as i16);
    }
    let mut bytes = Vec::with_capacity(out.len() * 2);
    for s in out {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_tokens_basic() {
        let final_tokens = vec![
            json!({"text":"Hello ", "is_final": true, "speaker": "1", "language": "en"}),
            json!({"text":"world", "is_final": true, "speaker": "1", "language": "en"}),
        ];
        let non_final =
            vec![json!({"text":"!", "is_final": false, "speaker": "1", "language": "en"})];
        let txt = render_tokens(&final_tokens, &non_final);
        assert!(txt.contains("Speaker 1:"));
        assert!(txt.contains("[en]"));
        assert!(txt.contains("Hello world!"));
    }

    #[test]
    fn test_to_pcm_identity_when_16k_mono() {
        let samples: Vec<i16> = (0..100).map(|i| i as i16).collect();
        let bytes = to_pcm16_mono_16k(&samples, 1, 16_000);
        assert_eq!(bytes.len(), samples.len() * 2);
        // First few bytes match little endian of samples
        assert_eq!(bytes[0], (0i16).to_le_bytes()[0]);
        assert_eq!(bytes[2], (1i16).to_le_bytes()[0]);
    }

    #[test]
    fn test_to_pcm_downmix_stereo() {
        // Two-channel interleaved: L=1000, R=3000 -> avg=2000
        let samples: Vec<i16> = vec![1000, 3000, 1000, 3000, 1000, 3000];
        let bytes = to_pcm16_mono_16k(&samples, 2, 16_000);
        assert_eq!(bytes.len(), 3 * 2);
        let v = i16::from_le_bytes([bytes[0], bytes[1]]);
        assert_eq!(v, 2000);
    }

    #[test]
    fn test_to_pcm_resample_8k_to_16k() {
        let samples: Vec<i16> = (0..80).map(|i| (i * 100) as i16).collect();
        let bytes = to_pcm16_mono_16k(&samples, 1, 8_000);
        // Expect roughly doubled samples (linear interpolation)
        assert!(bytes.len() >= samples.len() * 2);
    }
}
