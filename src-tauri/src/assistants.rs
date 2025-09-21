use crate::utils::log_to_file;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assistant {
    pub id: String,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub gate_instructions: String,
    #[serde(default)]
    pub output_policy: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AssistantsConfig {
    assistants: Vec<Assistant>,
    default_assistant: String,
}

pub struct AssistantManager {
    assistants: HashMap<String, Assistant>,
    default_id: String,
}

impl AssistantManager {
    pub fn empty() -> Self {
        Self {
            assistants: HashMap::new(),
            default_id: "".to_string(),
        }
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let config_path = path.as_ref();

        if !config_path.exists() {
            return Err(format!(
                "Assistants config file not found at: {:?}",
                config_path
            ));
        }

        match fs::read_to_string(config_path) {
            Ok(content) => {
                match serde_json::from_str::<AssistantsConfig>(&content) {
                    Ok(config) => {
                        if config.assistants.is_empty() {
                            return Err("No assistants defined in config file".to_string());
                        }

                        let mut assistants = HashMap::new();
                        for assistant in config.assistants {
                            if assistant.id.is_empty() {
                                return Err("Assistant with empty ID found in config".to_string());
                            }
                            if assistant.name.is_empty() {
                                return Err(format!("Assistant '{}' has empty name", assistant.id));
                            }
                            if assistant.system_prompt.is_empty() {
                                return Err(format!(
                                    "Assistant '{}' has empty system_prompt",
                                    assistant.id
                                ));
                            }
                            assistants.insert(assistant.id.clone(), assistant);
                        }

                        // Validate default_assistant exists
                        if !assistants.contains_key(&config.default_assistant) {
                            return Err(format!(
                                "Default assistant '{}' not found in assistants list",
                                config.default_assistant
                            ));
                        }

                        log_to_file(&format!(
                            "Successfully loaded {} assistants from config",
                            assistants.len()
                        ));

                        Ok(Self {
                            assistants,
                            default_id: config.default_assistant,
                        })
                    }
                    Err(e) => Err(format!("Failed to parse assistants config JSON: {}", e)),
                }
            }
            Err(e) => Err(format!("Failed to read assistants config file: {}", e)),
        }
    }

    pub fn get_assistant(&self, id: &str) -> Option<&Assistant> {
        self.assistants.get(id)
    }

    pub fn get_default_assistant(&self) -> &Assistant {
        self.assistants
            .get(&self.default_id)
            .unwrap_or_else(|| self.assistants.values().next().unwrap())
    }

    pub fn list_assistants(&self) -> Vec<&Assistant> {
        let mut assistants: Vec<&Assistant> = self.assistants.values().collect();
        assistants.sort_by(|a, b| a.name.cmp(&b.name));
        assistants
    }

    pub fn get_default_id(&self) -> &str {
        &self.default_id
    }
}
