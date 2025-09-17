use serde::{Deserialize, Serialize};
use crate::utils::log_to_file;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OpenAIOptions {
  pub api_key: String,
  #[serde(default = "default_model")]
  pub model: String,
  #[serde(default = "default_system_prompt")]
  pub system_prompt: String,
}

fn default_model() -> String {
  "gpt-4.1".into()
}

fn default_system_prompt() -> String {
  "You are an AI assistant that analyzes conversations and answers questions in the language that conversation is going on".into()
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
  role: String,
  content: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
  model: String,
  messages: Vec<ChatMessage>,
  max_tokens: u32,
  temperature: f32,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
  message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
  choices: Vec<ChatChoice>,
}

pub async fn analyze_conversation(opts: OpenAIOptions, transcript: String) -> Result<String, String> {
  if transcript.trim().is_empty() {
    return Ok("No conversation to analyze yet.".to_string());
  }

  log_to_file(&format!("OpenAI: Analyzing transcript of {} chars", transcript.len()));

  let system_prompt = opts.system_prompt.as_str();

  let user_prompt = format!("Please analyze this conversation transcript and answer recent question\n\n{}", transcript);

  let request = ChatRequest {
    model: opts.model,
    messages: vec![
      ChatMessage {
        role: "system".to_string(),
        content: system_prompt.to_string(),
      },
      ChatMessage {
        role: "user".to_string(),
        content: user_prompt,
      },
    ],
    max_tokens: 500,
    temperature: 0.0,
  };

  let client = reqwest::Client::new();

  match client
    .post("https://api.openai.com/v1/chat/completions")
    .header("Authorization", format!("Bearer {}", opts.api_key))
    .header("Content-Type", "application/json")
    .json(&request)
    .send()
    .await
  {
    Ok(response) => {
      let status = response.status();
      if !status.is_success() {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        log_to_file(&format!("OpenAI: API error {}: {}", status, error_text));
        return Err(format!("OpenAI API error: {}", status));
      }

      match response.json::<ChatResponse>().await {
        Ok(chat_response) => {
          if let Some(choice) = chat_response.choices.first() {
            let analysis = &choice.message.content;
            log_to_file(&format!("OpenAI: Generated analysis of {} chars", analysis.len()));
            Ok(analysis.clone())
          } else {
            Err("No response from OpenAI".to_string())
          }
        }
        Err(e) => {
          log_to_file(&format!("OpenAI: JSON parse error: {}", e));
          Err(format!("Failed to parse OpenAI response: {}", e))
        }
      }
    }
    Err(e) => {
      log_to_file(&format!("OpenAI: Request error: {}", e));
      Err(format!("Failed to connect to OpenAI: {}", e))
    }
  }
}

#[derive(Debug, Deserialize)]
struct ModelData {
  id: String,
  object: String,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
  data: Vec<ModelData>,
}

pub async fn get_available_models(api_key: String) -> Result<Vec<String>, String> {
  if api_key.trim().is_empty() {
    return Err("OpenAI API key is required".to_string());
  }

  log_to_file("OpenAI: Fetching available models");

  let client = reqwest::Client::new();

  match client
    .get("https://api.openai.com/v1/models")
    .header("Authorization", format!("Bearer {}", api_key))
    .send()
    .await
  {
    Ok(response) => {
      let status = response.status();
      if !status.is_success() {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        log_to_file(&format!("OpenAI: Models API error {}: {}", status, error_text));
        return Err(format!("OpenAI API error: {}", status));
      }

      match response.json::<ModelsResponse>().await {
        Ok(models_response) => {
          let mut models: Vec<String> = models_response
            .data
            .into_iter()
            .filter(|model| {
              // Filter to only include relevant chat models
              model.id.starts_with("gpt-") ||
              model.id == "o1-preview" ||
              model.id == "o1-mini"
            })
            .map(|model| model.id)
            .collect();

          // Sort models with newer/better ones first
          models.sort_by(|a, b| {
            let order_a = get_model_priority(a);
            let order_b = get_model_priority(b);
            order_a.cmp(&order_b)
          });

          log_to_file(&format!("OpenAI: Found {} relevant models", models.len()));
          Ok(models)
        }
        Err(e) => {
          log_to_file(&format!("OpenAI: JSON parse error: {}", e));
          Err(format!("Failed to parse OpenAI models response: {}", e))
        }
      }
    }
    Err(e) => {
      log_to_file(&format!("OpenAI: Models request error: {}", e));
      Err(format!("Failed to connect to OpenAI: {}", e))
    }
  }
}

fn get_model_priority(model: &str) -> u32 {
  match model {
    "o1-preview" => 1,
    "o1-mini" => 2,
    "gpt-4.1-nano" => 3,
    "gpt-4.1" => 3,
    s if s.starts_with("gpt-4") => 4,
    s if s.starts_with("gpt-3.5") => 5,
    _ => 6,
  }
}

// Lightweight gating API: decide whether to run a full analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GateOptions {
  pub api_key: String,
  #[serde(default = "default_gate_model")]
  pub model: String,
  #[serde(default = "default_system_prompt")]
  pub main_system_prompt: String,
  #[serde(default = "default_gate_instructions")]
  pub gate_instructions: String,
}

fn default_gate_model() -> String {
  // Prefer a lightweight model; fall back to a common one
  // Note: available models depend on the account
  "gpt-4.1-nano".into()
}

fn default_gate_instructions() -> String {
  "Run when there is new, materially different user intent, a new completed question/sentence, or when prior output no longer fits. Skip for partial/unstable ASR text, trivial edits, or small punctuation changes.".into()
}

#[derive(Debug, Deserialize)]
pub struct GateJson {
  pub run: bool,
  pub reason: Option<String>,
  pub confidence: Option<f32>,
}

pub async fn should_run_gate(opts: GateOptions, current_transcript: String, previous_transcript: String, last_output: Option<String>) -> Result<GateJson, String> {
  if current_transcript.trim().is_empty() {
    return Ok(GateJson { run: false, reason: Some("Empty transcript".into()), confidence: Some(1.0) });
  }

  // Very compact prompt to keep tokens low
  let system_prompt = format!(
    "You decide if the main assistant should re-run.\nRole: {}\nRules: {}. Respond with strict JSON only.",
    opts.main_system_prompt,
    opts.gate_instructions
  );

  let user_prompt = format!(
    "Current transcript:\n{}\n\nPrevious transcript:\n{}\n\nLast output (optional):\n{}\n\nReturn JSON: {{\"run\": boolean, \"reason\": string, \"confidence\": number}}",
    current_transcript,
    previous_transcript,
    last_output.unwrap_or_default()
  );

  let request = ChatRequest {
    model: opts.model,
    messages: vec![
      ChatMessage { role: "system".into(), content: system_prompt },
      ChatMessage { role: "user".into(), content: user_prompt },
    ],
    max_tokens: 120,
    temperature: 0.0,
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
      Ok(g) => Ok(g),
      Err(e) => {
        log_to_file(&format!("OpenAI(Gate): JSON parse error: {} | content: {}", e, content));
        // Heuristic fallback: trigger if there is a growth of >= 50 chars and ends with sentence punctuation
        let ends = current_transcript.trim_end().ends_with(['.', '!', '?']);
        let growth = current_transcript.len().saturating_sub(previous_transcript.len());
        Ok(GateJson { run: ends && growth >= 50, reason: Some("Fallback heuristic".into()), confidence: Some(0.3) })
      }
    }
  } else {
    Err("No response from OpenAI".into())
  }
}
