use crate::{assistants::render_user_prompt, gate::GateJson, openai, utils::log_to_file};
use openrouter_rs::{
    api::chat::{ChatCompletionRequest, Message},
    api::credits::CreditsData,
    types::Role,
    OpenRouterClient,
};
use serde::Serialize;

const HTTP_REFERER: &str = "https://neuro-note.local";
const X_TITLE: &str = "Neuro Note";

#[derive(Debug, Clone, Serialize)]
pub struct CreditSummary {
    pub total_credits: f64,
    pub total_usage: f64,
}

impl From<CreditsData> for CreditSummary {
    fn from(value: CreditsData) -> Self {
        Self {
            total_credits: value.total_credits,
            total_usage: value.total_usage,
        }
    }
}

pub fn build_client(api_key: &str) -> Result<OpenRouterClient, String> {
    OpenRouterClient::builder()
        .api_key(api_key)
        .http_referer(HTTP_REFERER)
        .x_title(X_TITLE)
        .build()
        .map_err(|e| {
            log_to_file(&format!("OpenRouter: Failed to build client: {}", e));
            format!("OpenRouter client error: {}", e)
        })
}

pub fn compose_messages(
    system_prompt: &str,
    output_policy: &str,
    user_prompt_template: &str,
    transcript: &str,
    last_output: Option<&str>,
) -> Vec<Message> {
    let effective_system_prompt = if output_policy.trim().is_empty() {
        system_prompt.to_string()
    } else {
        format!("{}\n\n{}", system_prompt, output_policy)
    };

    let mut messages = vec![Message::new(Role::System, &effective_system_prompt)];

    if let Some(prev) = last_output {
        if !prev.trim().is_empty() {
            messages.push(Message::new(
                Role::Assistant,
                &format!("Previous assistant answer (for context):\n{}", prev),
            ));
        }
    }

    let user_prompt = render_user_prompt(user_prompt_template, transcript);

    messages.push(Message::new(Role::User, &user_prompt));
    messages
}

pub fn build_chat_request(
    model: &str,
    messages: Vec<Message>,
    max_tokens: u32,
) -> Result<ChatCompletionRequest, String> {
    if let Some(system_msg) = messages.iter().find(|m| matches!(m.role, Role::System)) {
        let user_msg = messages.iter().rev().find(|m| matches!(m.role, Role::User));
        log_to_file(&format!(
            "OpenRouter(Main): Prompt model={} system=<<<{}>>> user=<<<{}>>>",
            model,
            system_msg.content,
            user_msg.map(|m| m.content.as_str()).unwrap_or("")
        ));
    }

    let temperature = openai::temperature_for_model(model, 0.0) as f64;
    ChatCompletionRequest::builder()
        .model(model.to_string())
        .messages(messages)
        .max_tokens(max_tokens)
        .temperature(temperature)
        .build()
        .map_err(|e| {
            log_to_file(&format!("OpenRouter: Failed to build chat request: {}", e));
            format!("OpenRouter request error: {}", e)
        })
}

pub async fn list_models(api_key: &str) -> Result<Vec<String>, String> {
    let client = build_client(api_key)?;
    match client.list_models().await {
        Ok(mut models) => {
            let mut ids: Vec<String> = models.drain(..).map(|model| model.id).collect();
            ids.sort();
            Ok(ids)
        }
        Err(e) => {
            log_to_file(&format!("OpenRouter: Failed to list models: {}", e));
            Err(format!("Failed to fetch OpenRouter models: {}", e))
        }
    }
}

pub async fn get_credits(api_key: &str) -> Result<CreditSummary, String> {
    let client = build_client(api_key)?;
    match client.get_credits().await {
        Ok(data) => Ok(data.into()),
        Err(e) => {
            log_to_file(&format!("OpenRouter: Failed to fetch credits: {}", e));
            Err(format!("Failed to fetch OpenRouter credits: {}", e))
        }
    }
}

pub async fn run_gate(
    api_key: &str,
    model: &str,
    main_system_prompt: &str,
    gate_instructions: &str,
    current_transcript: String,
    previous_transcript: String,
    last_output: Option<String>,
) -> Result<GateJson, String> {
    if current_transcript.trim().is_empty() {
        log_to_file("OpenRouter(Gate): Skip — empty transcript");
        return Ok(GateJson {
            run: false,
            instruction: Some("NOT_NEEDED".into()),
            reason: Some("Empty transcript".into()),
            confidence: Some(1.0),
        });
    }

    let system_prompt = format!(
        "You decide if the main assistant should re-run.\nRole: {}\nRules: {}\nOutput MUST be STRICT JSON with keys: run(boolean), instruction(NEEDED|NOT_NEEDED), reason(string), confidence(number). No extra text.",
        main_system_prompt,
        gate_instructions
    );

    let last_out_len = last_output.as_ref().map(|s| s.len()).unwrap_or(0);
    let last_out_text = last_output.unwrap_or_default();

    let user_prompt = format!(
        "Current transcript:\n{}\n\nPrevious transcript:\n{}\n\nLast output (optional):\n{}\n\nReturn ONLY this JSON: {{\"run\": boolean, \"instruction\": \"NEEDED\"|\"NOT_NEEDED\", \"reason\": string, \"confidence\": number}}",
        current_transcript,
        previous_transcript,
        last_out_text
    );

    log_to_file(&format!(
        "OpenRouter(Gate): Prompt system=<<<{}>>> user=<<<{}>>>",
        system_prompt, user_prompt
    ));

    log_to_file(&format!(
        "OpenRouter(Gate): Request model={} current_len={} previous_len={} last_out_len={}",
        model,
        current_transcript.len(),
        previous_transcript.len(),
        last_out_len
    ));

    let mut messages = Vec::with_capacity(2);
    messages.push(Message::new(Role::System, &system_prompt));
    messages.push(Message::new(Role::User, &user_prompt));

    let request = ChatCompletionRequest::builder()
        .model(model.to_string())
        .messages(messages)
        .max_tokens(120)
        .temperature(0.0)
        .build()
        .map_err(|e| {
            log_to_file(&format!("OpenRouter(Gate): Failed to build request: {}", e));
            format!("OpenRouter gate request error: {}", e)
        })?;

    let client = build_client(api_key)?;
    let response = client.send_chat_completion(&request).await.map_err(|e| {
        log_to_file(&format!("OpenRouter(Gate): API error: {}", e));
        format!("OpenRouter gate API error: {}", e)
    })?;

    if let Some(choice) = response.choices.first() {
        let content = choice.content().unwrap_or_default().trim().to_string();
        log_to_file(&format!("OpenRouter(Gate): Raw response=<<<{}>>>", content));

        match serde_json::from_str::<GateJson>(&content) {
            Ok(mut g) => {
                if g.instruction.is_none() {
                    g.instruction = Some(if g.run {
                        "NEEDED".into()
                    } else {
                        "NOT_NEEDED".into()
                    });
                }
                log_to_file(&format!(
                    "OpenRouter(Gate): Decision run={} instruction={:?} confidence={:?} reason={}",
                    g.run,
                    g.instruction,
                    g.confidence,
                    g.reason.clone().unwrap_or_default()
                ));
                Ok(g)
            }
            Err(e) => {
                log_to_file(&format!(
                    "OpenRouter(Gate): JSON parse error: {} | content: {}",
                    e, content
                ));
                let ends = current_transcript.trim_end().ends_with(['.', '!', '?']);
                let growth = current_transcript
                    .len()
                    .saturating_sub(previous_transcript.len());
                let run = ends && growth >= 50;
                log_to_file(&format!(
                    "OpenRouter(Gate): Fallback decision run={} ends_sentence={} growth_chars={}",
                    run, ends, growth
                ));
                Ok(GateJson {
                    run,
                    instruction: Some(if run {
                        "NEEDED".into()
                    } else {
                        "NOT_NEEDED".into()
                    }),
                    reason: Some("Fallback heuristic".into()),
                    confidence: Some(0.3),
                })
            }
        }
    } else {
        Err("No response from OpenRouter".into())
    }
}
