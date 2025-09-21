use crate::{openai, utils::log_to_file};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone)]
pub struct GatePrompt {
    pub system_prompt: String,
    pub user_prompt: String,
    pub current_len: usize,
    pub previous_len: usize,
    pub last_output_len: usize,
}

pub fn prepare_gate_prompt(
    provider_label: &str,
    main_system_prompt: &str,
    gate_instructions: &str,
    current_transcript: &str,
    previous_transcript: &str,
    last_output: Option<&str>,
) -> Result<GatePrompt, GateJson> {
    if current_transcript.trim().is_empty() {
        log_to_file(&format!(
            "{}(Gate): Skip — empty transcript",
            provider_label
        ));
        return Err(GateJson {
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

    let (last_output_text, last_output_len) = match last_output {
        Some(text) => (text, text.len()),
        None => ("", 0),
    };

    let user_prompt = format!(
        "Current transcript:\n{}\n\nPrevious transcript:\n{}\n\nLast output (optional):\n{}\n\nReturn ONLY this JSON: {{\"run\": boolean, \"instruction\": \"NEEDED\"|\"NOT_NEEDED\", \"reason\": string, \"confidence\": number}}",
        current_transcript,
        previous_transcript,
        last_output_text
    );

    Ok(GatePrompt {
        system_prompt,
        user_prompt,
        current_len: current_transcript.len(),
        previous_len: previous_transcript.len(),
        last_output_len,
    })
}

pub fn log_gate_prompt(provider_label: &str, model: &str, prompt: &GatePrompt) {
    log_to_file(&format!(
        "{}(Gate): Prompt system=<<<{}>>> user=<<<{}>>>",
        provider_label, prompt.system_prompt, prompt.user_prompt
    ));

    log_to_file(&format!(
        "{}(Gate): Request model={} current_len={} previous_len={} last_out_len={}",
        provider_label, model, prompt.current_len, prompt.previous_len, prompt.last_output_len
    ));
}

pub fn interpret_gate_response(
    provider_label: &str,
    content: &str,
    current_transcript: &str,
    previous_transcript: &str,
) -> GateJson {
    match serde_json::from_str::<GateJson>(content) {
        Ok(mut gate) => {
            if gate.instruction.is_none() {
                gate.instruction = Some(if gate.run {
                    "NEEDED".into()
                } else {
                    "NOT_NEEDED".into()
                });
            }

            log_to_file(&format!(
                "{}(Gate): Decision run={} instruction={:?} confidence={:?} reason={}",
                provider_label,
                gate.run,
                gate.instruction,
                gate.confidence,
                gate.reason.clone().unwrap_or_default()
            ));

            gate
        }
        Err(err) => {
            log_to_file(&format!(
                "{}(Gate): JSON parse error: {} | content: {}",
                provider_label, err, content
            ));

            let ends_sentence = current_transcript.trim_end().ends_with(['.', '!', '?']);
            let growth = current_transcript
                .len()
                .saturating_sub(previous_transcript.len());
            let run = ends_sentence && growth >= 50;

            log_to_file(&format!(
                "{}(Gate): Fallback decision run={} ends_sentence={} growth_chars={}",
                provider_label, run, ends_sentence, growth
            ));

            GateJson {
                run,
                instruction: Some(if run {
                    "NEEDED".into()
                } else {
                    "NOT_NEEDED".into()
                }),
                reason: Some("Fallback heuristic".into()),
                confidence: Some(0.3),
            }
        }
    }
}

pub async fn should_run_gate(
    opts: GateOptions,
    current_transcript: String,
    previous_transcript: String,
    last_output: Option<String>,
) -> Result<GateJson, String> {
    let prompt = match prepare_gate_prompt(
        "OpenAI",
        &opts.main_system_prompt,
        &opts.gate_instructions,
        &current_transcript,
        &previous_transcript,
        last_output.as_deref(),
    ) {
        Ok(prompt) => prompt,
        Err(skip) => return Ok(skip),
    };

    log_gate_prompt("OpenAI", &opts.model, &prompt);

    let GatePrompt {
        system_prompt,
        user_prompt,
        ..
    } = prompt;

    let model_id = opts.model.clone();
    let temp = openai::temperature_for_model(&model_id, 0.0);
    let request = ChatRequest {
        model: model_id,
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: system_prompt,
            },
            ChatMessage {
                role: "user".into(),
                content: user_prompt,
            },
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
        let error_text = resp
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        log_to_file(&format!(
            "OpenAI(Gate): API error {}: {}",
            status, error_text
        ));
        return Err(format!("OpenAI API error: {}", status));
    }

    let chat: ChatResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse OpenAI response: {}", e))?;

    if let Some(choice) = chat.choices.first() {
        let content = choice.message.content.trim().to_string();
        log_to_file(&format!("OpenAI(Gate): Raw response=<<<{}>>>", content));
        Ok(interpret_gate_response(
            "OpenAI",
            &content,
            &current_transcript,
            &previous_transcript,
        ))
    } else {
        Err("No response from OpenAI".into())
    }
}
