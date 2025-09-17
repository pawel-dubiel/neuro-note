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
  #[serde(default)]
  pub output_policy: String,
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
  max_completion_tokens: u32,
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

fn temperature_for_model(model: &str, default: f32) -> f32 {
  if model.starts_with("gpt-5") || model.contains("gpt-5") {
    1.0
  } else {
    default
  }
}

pub async fn analyze_conversation(opts: OpenAIOptions, transcript: String, last_output: Option<String>) -> Result<String, String> {
  if transcript.trim().is_empty() {
    return Ok("No conversation to analyze yet.".to_string());
  }

  let last_out_len = last_output.as_ref().map(|s| s.len()).unwrap_or(0);
  log_to_file(&format!(
    "OpenAI(Model): Request model={} transcript_len={} last_out_len={}",
    opts.model,
    transcript.len(),
    last_out_len
  ));

  // Compose system prompt with any assistant-defined output policy from config
  let effective_system_prompt = if opts.output_policy.trim().is_empty() {
    opts.system_prompt.clone()
  } else {
    format!("{}\n\n{}", opts.system_prompt, opts.output_policy)
  };

  let user_prompt = format!(
    "Analyze the latest user intent in the transcript.\n- If a previous assistant answer is shown above, DO NOT repeat it.\n- Only add new information, corrections, or next steps relevant to the newest utterances.\n- Be concise and avoid duplication.\n\n{}",
    transcript
  );

  let mut messages = vec![
    ChatMessage { role: "system".to_string(), content: effective_system_prompt },
  ];
  if let Some(prev) = last_output {
    if !prev.trim().is_empty() {
      messages.push(ChatMessage { role: "assistant".to_string(), content: format!("Previous assistant answer (for context):\n{}", prev) });
    }
  }
  messages.push(ChatMessage { role: "user".to_string(), content: user_prompt });

  let temp = temperature_for_model(&opts.model, 0.0);
  let request = ChatRequest { model: opts.model, messages, max_completion_tokens: 500, temperature: temp };

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
            log_to_file(&format!(
              "OpenAI(Model): Response model={} analysis_len={}",
              request.model,
              analysis.len()
            ));
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
