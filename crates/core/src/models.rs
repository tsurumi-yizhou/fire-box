use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;

/// Raw data structure returned by the models.dev API
#[derive(Debug, Deserialize)]
pub struct ModelsDevResponse {
    #[serde(flatten)]
    pub providers: HashMap<String, ProviderData>,
}

#[derive(Debug, Deserialize)]
pub struct ProviderData {
    #[serde(default)]
    pub models: HashMap<String, ModelMetadata>,
}

/// Model metadata as returned by the API
#[derive(Debug, Deserialize)]
pub struct ModelMetadata {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub attachment: bool,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub tool_call: bool,
    #[serde(default)]
    pub structured_output: bool,
    #[serde(default)]
    pub temperature: bool,
    #[serde(default)]
    pub modalities: Option<ApiModalities>,
    #[serde(default)]
    pub open_weights: bool,
}

#[derive(Debug, Deserialize)]
pub struct ApiModalities {
    #[serde(default)]
    pub input: Vec<String>,
    #[serde(default)]
    pub output: Vec<String>,
}

/// Transformed model capability information
#[derive(Debug, Clone)]
pub struct ModelCapabilities {
    #[allow(dead_code)]
    pub id: String,
    pub name: String,
    #[allow(dead_code)]
    pub family: Option<String>,
    pub capabilities: Capabilities,
    pub input: Modalities,
    #[allow(dead_code)]
    pub output: Modalities,
    #[allow(dead_code)]
    pub open_weights: bool,
}

#[derive(Debug, Clone)]
pub struct Capabilities {
    pub tool_call: bool,
    pub reasoning: bool,
    #[allow(dead_code)]
    pub structured_output: bool,
    #[allow(dead_code)]
    pub temperature: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Modalities {
    pub text: bool,
    pub image: bool,
    #[allow(dead_code)]
    pub audio: bool,
    #[allow(dead_code)]
    pub video: bool,
    pub pdf: bool,
}

/// Model metadata registry
#[derive(Debug, Default)]
pub struct ModelRegistry {
    models: HashMap<String, ModelCapabilities>,
}

impl ModelRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Download and load model metadata from models.dev API
    pub async fn load_from_models_dev() -> Result<Self> {
        let url = "https://models.dev/api.json";

        let response = reqwest::get(url)
            .await
            .context("Failed to fetch models.dev API")?;

        let data: ModelsDevResponse = response
            .json()
            .await
            .context("Failed to parse models.dev response")?;

        Ok(Self::from_api_data(data))
    }

    /// Build the registry from API data
    fn from_api_data(data: ModelsDevResponse) -> Self {
        let mut models = HashMap::new();

        for (_provider_id, provider) in data.providers {
            for (model_id, metadata) in provider.models {
                let capabilities = ModelCapabilities {
                    id: metadata.id.clone(),
                    name: metadata.name,
                    family: metadata.family,
                    capabilities: Capabilities {
                        tool_call: metadata.tool_call,
                        reasoning: metadata.reasoning,
                        structured_output: metadata.structured_output,
                        temperature: metadata.temperature,
                    },
                    input: metadata
                        .modalities
                        .as_ref()
                        .map(|m| Modalities::from_string_list(&m.input))
                        .unwrap_or_default(),
                    output: metadata
                        .modalities
                        .as_ref()
                        .map(|m| Modalities::from_string_list(&m.output))
                        .unwrap_or_default(),
                    open_weights: metadata.open_weights,
                };

                models.insert(model_id, capabilities);
            }
        }

        Self { models }
    }

    /// Query model metadata by model ID
    pub fn get(&self, model_id: &str) -> Option<&ModelCapabilities> {
        self.models.get(model_id)
    }

    /// Get the number of models
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// Check if the registry is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

impl Modalities {
    /// Create from a list of strings
    fn from_string_list(list: &[String]) -> Self {
        Self {
            text: list.iter().any(|s| s == "text"),
            image: list.iter().any(|s| s == "image"),
            audio: list.iter().any(|s| s == "audio"),
            video: list.iter().any(|s| s == "video"),
            pdf: list.iter().any(|s| s == "pdf"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modalities_from_string_list() {
        let list = vec!["text".to_string(), "image".to_string(), "pdf".to_string()];
        let modalities = Modalities::from_string_list(&list);

        assert!(modalities.text);
        assert!(modalities.image);
        assert!(modalities.pdf);
        assert!(!modalities.audio);
        assert!(!modalities.video);
    }

    #[test]
    fn test_modalities_default() {
        let modalities = Modalities::default();

        assert!(!modalities.text);
        assert!(!modalities.image);
        assert!(!modalities.audio);
        assert!(!modalities.video);
        assert!(!modalities.pdf);
    }

    #[tokio::test]
    async fn test_load_from_models_dev() {
        let registry = ModelRegistry::load_from_models_dev().await;
        assert!(registry.is_ok());

        let registry = registry.unwrap();
        assert!(!registry.is_empty());
        println!("Loaded {} models", registry.len());

        // Test lookup of a specific model
        if let Some(model) = registry.get("gpt-5.2") {
            println!("Found model: {} ({})", model.name, model.id);
            println!("  Tool call: {}", model.capabilities.tool_call);
            println!("  Reasoning: {}", model.capabilities.reasoning);
            println!(
                "  Input: text={}, image={}, audio={}, video={}, pdf={}",
                model.input.text,
                model.input.image,
                model.input.audio,
                model.input.video,
                model.input.pdf
            );
        }
    }
}
