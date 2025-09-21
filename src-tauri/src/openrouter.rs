use crate::{
    assistants::render_user_prompt,
    gate::{self, GateJson, GatePrompt},
    openai,
    utils::log_to_file,
};
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
    let prompt = match gate::prepare_gate_prompt(
        "OpenRouter",
        main_system_prompt,
        gate_instructions,
        &current_transcript,
        &previous_transcript,
        last_output.as_deref(),
    ) {
        Ok(prompt) => prompt,
        Err(skip) => return Ok(skip),
    };

    gate::log_gate_prompt("OpenRouter", model, &prompt);

    let GatePrompt {
        system_prompt,
        user_prompt,
        ..
    } = prompt;

    let messages = vec![
        Message::new(Role::System, &system_prompt),
        Message::new(Role::User, &user_prompt),
    ];

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

        Ok(gate::interpret_gate_response(
            "OpenRouter",
            &content,
            &current_transcript,
            &previous_transcript,
        ))
    } else {
        Err("No response from OpenRouter".into())
    }
}
