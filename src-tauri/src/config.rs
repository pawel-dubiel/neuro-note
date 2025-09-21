use crate::utils::log_to_file;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SonioxConfig {
    pub api_key: String,
    #[serde(default = "default_audio_format")]
    pub audio_format: String,
    #[serde(default = "default_translation")]
    pub translation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_gate_model")]
    pub gate_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum AiProvider {
    #[default]
    Openai,
    Openrouter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterConfig {
    pub api_key: String,
    #[serde(default = "default_openrouter_model")]
    pub model: String,
    #[serde(default = "default_openrouter_gate_model")]
    pub gate_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingConfig {
    #[serde(default = "default_format")]
    pub default_format: String,
    #[serde(default = "default_quality")]
    pub default_quality: String,
    #[serde(default = "default_auto_detect")]
    pub auto_detect_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIConfig {
    #[serde(default)]
    pub enable_soniox: bool,
    #[serde(default, alias = "enable_openai")]
    pub enable_ai: bool,
    #[serde(default = "default_assistant")]
    pub default_assistant: String,
    #[serde(default = "AiProvider::default")]
    pub ai_provider: AiProvider,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub soniox: SonioxConfig,
    pub openai: OpenAIConfig,
    #[serde(default)]
    pub openrouter: OpenRouterConfig,
    pub recording: RecordingConfig,
    pub ui: UIConfig,
}

// Default functions
fn default_audio_format() -> String {
    "pcm_s16le".to_string()
}
fn default_translation() -> String {
    "none".to_string()
}
fn default_model() -> String {
    "gpt-4.1".to_string()
}
fn default_gate_model() -> String {
    "gpt-4.1-nano".to_string()
}
fn default_openrouter_model() -> String {
    "deepseek/deepseek-chat-v3-0324:free".to_string()
}
fn default_openrouter_gate_model() -> String {
    "deepseek/deepseek-chat-v3-0324:free".to_string()
}
fn default_format() -> String {
    "mp3".to_string()
}
fn default_quality() -> String {
    "verylow".to_string()
}
fn default_auto_detect() -> bool {
    true
}
fn default_assistant() -> String {
    "general".to_string()
}

impl Default for SonioxConfig {
    fn default() -> Self {
        Self {
            api_key: "".to_string(),
            audio_format: default_audio_format(),
            translation: default_translation(),
        }
    }
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            api_key: "".to_string(),
            model: default_model(),
            gate_model: default_gate_model(),
        }
    }
}

impl Default for OpenRouterConfig {
    fn default() -> Self {
        Self {
            api_key: "".to_string(),
            model: default_openrouter_model(),
            gate_model: default_openrouter_gate_model(),
        }
    }
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            default_format: default_format(),
            default_quality: default_quality(),
            auto_detect_enabled: default_auto_detect(),
        }
    }
}

impl Default for UIConfig {
    fn default() -> Self {
        Self {
            enable_soniox: false,
            enable_ai: false,
            default_assistant: default_assistant(),
            ai_provider: AiProvider::default(),
        }
    }
}

pub struct ConfigManager;

impl ConfigManager {
    pub fn load_config() -> Result<AppConfig, String> {
        let config_path = "../config/config.local.json";

        if !Path::new(config_path).exists() {
            log_to_file(&format!("Config file not found at: {}", config_path));
            return Err(format!("Configuration file not found at: {}", config_path));
        }

        match fs::read_to_string(config_path) {
            Ok(content) => match serde_json::from_str::<AppConfig>(&content) {
                Ok(config) => {
                    log_to_file("Successfully loaded app configuration");
                    Ok(config)
                }
                Err(e) => {
                    let error = format!("Failed to parse config JSON: {}", e);
                    log_to_file(&error);
                    Err(error)
                }
            },
            Err(e) => {
                let error = format!("Failed to read config file: {}", e);
                log_to_file(&error);
                Err(error)
            }
        }
    }

    pub fn save_config(config: &AppConfig) -> Result<(), String> {
        let config_path = "../config/config.local.json";

        match serde_json::to_string_pretty(config) {
            Ok(json_content) => match fs::write(config_path, json_content) {
                Ok(()) => {
                    log_to_file("Successfully saved app configuration");
                    Ok(())
                }
                Err(e) => {
                    let error = format!("Failed to write config file: {}", e);
                    log_to_file(&error);
                    Err(error)
                }
            },
            Err(e) => {
                let error = format!("Failed to serialize config: {}", e);
                log_to_file(&error);
                Err(error)
            }
        }
    }

    pub fn create_default_config() -> Result<(), String> {
        let config = AppConfig::default();
        Self::save_config(&config)
    }
}
