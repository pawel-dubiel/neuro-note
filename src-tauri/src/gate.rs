use serde::{Deserialize, Serialize};
use crate::utils::log_to_file;

// Reuse chat request/response structures from a minimal local definition
// to keep this module self-contained and focused on gating.

#[derive(Debug, Serialize)]
struct ChatMessage {
  role: String,
  content: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
  model: String,
  messages: Vec<ChatMessage>,
  max_completion_tokens: u32,
  temperature: f32,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
  message: ChatMessageOut,
}

#[derive(Debug, Deserialize)]
struct ChatMessageOut {
  role: String,
  content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
  choices: Vec<ChatChoice>,
}

// Lightweight gating API: decide whether to run a full analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GateOptions {
  pub api_key: String,
  #[serde(default = "default_gate_model")]
  pub model: String,
  #[serde(default = "default_main_system_prompt")]
  pub main_system_prompt: String,
  #[serde(default = "default_gate_instructions")]
  pub gate_instructions: String,
}

fn default_gate_model() -> String {
  // Prefer a lightweight model; fall back to a common one
  // Note: available models depend on the account
  "gpt-4.1-nano".into()
}

fn default_main_system_prompt() -> String {
  "You are an AI assistant that analyzes conversations and answers questions in the language that conversation is going on".into()
}

fn default_gate_instructions() -> String {
  // Use assistants.json to override; this is a safe default
  "Run when there is new, materially different user intent, a new completed question/sentence, or when prior output no longer fits. Skip for partial/unstable ASR text, trivial edits, or small punctuation changes. Also skip if the last assistant output already addresses the current prompt adequately or if the user's thoughts seem unfinished.".into()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GateJson {
  pub run: bool,
  pub instruction: Option<String>,
  pub reason: Option<String>,
  pub confidence: Option<f32>,
}

pub async fn should_run_gate(
  opts: GateOptions,
  current_transcript: String,
  previous_transcript: String,
  last_output: Option<String>,
) -> Result<GateJson, String> {
  if current_transcript.trim().is_empty() {
    log_to_file("OpenAI(Gate): Skip — empty transcript");
    return Ok(GateJson { run: false, instruction: Some("NOT_NEEDED".into()), reason: Some("Empty transcript".into()), confidence: Some(1.0) });
  }

  // Compact prompt to keep tokens low; explicitly require a clear instruction output
  let system_prompt = format!(
    "You decide if the main assistant should re-run.\nRole: {}\nRules: {}\nOutput MUST be STRICT JSON with keys: run(boolean), instruction(NEEDED|NOT_NEEDED), reason(string), confidence(number). No extra text.",
    opts.main_system_prompt,
    opts.gate_instructions
  );

  // Safely derive last output values without moving the Option more than once
  let last_out_len = last_output.as_ref().map(|s| s.len()).unwrap_or(0);
  let last_out_text = last_output.unwrap_or_default();

  let user_prompt = format!(
    "Current transcript:\n{}\n\nPrevious transcript:\n{}\n\nLast output (optional):\n{}\n\nReturn ONLY this JSON: {{\"run\": boolean, \"instruction\": \"NEEDED\"|\"NOT_NEEDED\", \"reason\": string, \"confidence\": number}}",
    current_transcript,
    previous_transcript,
    last_out_text
  );

  log_to_file(&format!(
    "OpenAI(Gate): Request model={} current_len={} previous_len={} last_out_len={}",
    opts.model,
    current_transcript.len(),
    previous_transcript.len(),
    last_out_len
  ));

  let model_id = opts.model.clone();
  let temp = temperature_for_model(&model_id, 0.0);
  let request = ChatRequest {
    model: model_id,
    messages: vec![
      ChatMessage { role: "system".into(), content: system_prompt },
      ChatMessage { role: "user".into(), content: user_prompt },
    ],
    max_completion_tokens: 120,
    temperature: temp,
  };

  let client = reqwest::Client::new();
  let resp = client
    .post("https://api.openai.com/v1/chat/completions")
    .header("Authorization", format!("Bearer {}", opts.api_key))
    .header("Content-Type", "application/json")
    .json(&request)
    .send()
    .await
    .map_err(|e| format!("Failed to connect to OpenAI: {}", e))?;

  let status = resp.status();
  if !status.is_success() {
    let error_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
    log_to_file(&format!("OpenAI(Gate): API error {}: {}", status, error_text));
    return Err(format!("OpenAI API error: {}", status));
  }

  let chat: ChatResponse = resp
    .json()
    .await
    .map_err(|e| format!("Failed to parse OpenAI response: {}", e))?;

  if let Some(choice) = chat.choices.first() {
    let content = choice.message.content.trim();
    // Attempt to parse strict JSON; if fails, fallback to simple heuristic
    match serde_json::from_str::<GateJson>(content) {
      Ok(mut g) => {
        if g.instruction.is_none() {
          g.instruction = Some(if g.run { "NEEDED".into() } else { "NOT_NEEDED".into() });
        }
        log_to_file(&format!(
          "OpenAI(Gate): Decision run={} instruction={:?} confidence={:?} reason={}",
          g.run,
          g.instruction,
          g.confidence,
          g.reason.clone().unwrap_or_default()
        ));
        Ok(g)
      },
      Err(e) => {
        log_to_file(&format!("OpenAI(Gate): JSON parse error: {} | content: {}", e, content));
        // Heuristic fallback: trigger if there is a growth of >= 50 chars and ends with sentence punctuation
        let ends = current_transcript.trim_end().ends_with(['.', '!', '?']);
        let growth = current_transcript.len().saturating_sub(previous_transcript.len());
        let run = ends && growth >= 50;
        log_to_file(&format!(
          "OpenAI(Gate): Fallback decision run={} ends_sentence={} growth_chars={}",
          run, ends, growth
        ));
        Ok(GateJson { run, instruction: Some(if run { "NEEDED".into() } else { "NOT_NEEDED".into() }), reason: Some("Fallback heuristic".into()), confidence: Some(0.3) })
      }
    }
  } else {
    Err("No response from OpenAI".into())
  }
}
fn temperature_for_model(model: &str, default: f32) -> f32 {
  if model.starts_with("gpt-5") || model.contains("gpt-5") {
    1.0
  } else {
    default
  }
}
