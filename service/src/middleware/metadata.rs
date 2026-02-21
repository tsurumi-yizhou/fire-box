//! Metadata management for AI providers and models.
//!
//! This module handles downloading and parsing vendor/model metadata from
//! https://models.dev/api.json

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Base URL for metadata API
const METADATA_API_URL: &str = "https://models.dev/api.json";

/// Root structure of the API response - a map of vendor IDs to vendor data
pub type MetadataResponse = HashMap<String, Vendor>;

/// Vendor/Provider metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Vendor {
    /// Unique vendor identifier
    pub id: String,
    /// Required environment variable names for API authentication
    pub env: Vec<String>,
    /// NPM package name for SDK integration
    pub npm: String,
    /// Base API endpoint URL
    pub api: String,
    /// Human-readable vendor name
    pub name: String,
    /// Documentation URL
    pub doc: String,
    /// Models offered by this vendor
    pub models: HashMap<String, Model>,
}

/// Model metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Model {
    /// Unique model identifier
    pub id: String,
    /// Human-readable model name
    pub name: String,
    /// Model family/category (e.g., "qwen", "gpt", "claude")
    pub family: String,
    /// Supports file attachments
    #[serde(default)]
    pub attachment: bool,
    /// Supports reasoning/thinking capabilities
    #[serde(default)]
    pub reasoning: bool,
    /// Supports function/tool calling
    #[serde(default)]
    pub tool_call: bool,
    /// Reasoning output configuration
    #[serde(default)]
    pub interleaved: Option<InterleavedConfig>,
    /// Supports structured JSON output
    #[serde(default)]
    pub structured_output: bool,
    /// Temperature parameter is configurable
    #[serde(default)]
    pub temperature: bool,
    /// Knowledge cutoff date (e.g., "2025-04")
    #[serde(default)]
    pub knowledge: Option<String>,
    /// Model release date (ISO format)
    #[serde(default)]
    pub release_date: Option<String>,
    /// Last update date (ISO format)
    #[serde(default)]
    pub last_updated: Option<String>,
    /// Input/output modality support
    #[serde(default)]
    pub modalities: Option<Modalities>,
    /// Open weights model
    #[serde(default)]
    pub open_weights: bool,
    /// Pricing information (per million tokens, USD)
    #[serde(default)]
    pub cost: Option<Pricing>,
    /// Token limits
    #[serde(default)]
    pub limit: Option<Limits>,
}

/// Reasoning output configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InterleavedConfig {
    /// Field name for reasoning content
    pub field: String,
}

/// Input/output modality support
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Modalities {
    /// Supported input types
    #[serde(default)]
    pub input: Vec<String>,
    /// Supported output types
    #[serde(default)]
    pub output: Vec<String>,
}

/// Pricing information (per million tokens, USD)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Pricing {
    /// Input token cost
    #[serde(default)]
    pub input: f64,
    /// Output token cost
    #[serde(default)]
    pub output: f64,
    /// Cache read cost (optional)
    #[serde(default)]
    pub cache_read: Option<f64>,
    /// Cache write cost (optional)
    #[serde(default)]
    pub cache_write: Option<f64>,
    /// Reasoning tokens cost (optional)
    #[serde(default)]
    pub reasoning: Option<f64>,
    /// Audio input cost (optional)
    #[serde(default)]
    pub input_audio: Option<f64>,
    /// Audio output cost (optional)
    #[serde(default)]
    pub output_audio: Option<f64>,
}

/// Token limits
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Limits {
    /// Maximum context window size
    #[serde(default)]
    pub context: u64,
    /// Maximum input tokens (optional)
    #[serde(default)]
    pub input: Option<u64>,
    /// Maximum output tokens
    #[serde(default)]
    pub output: u64,
}

/// Metadata manager for caching and accessing vendor/model information
#[derive(Debug, Default)]
pub struct MetadataManager {
    /// Cached metadata response
    data: Option<MetadataResponse>,
}

impl MetadataManager {
    /// Create a new metadata manager
    pub fn new() -> Self {
        Self { data: None }
    }

    /// Download metadata from the API
    pub async fn download(&mut self) -> Result<&MetadataResponse> {
        log::info!("Downloading metadata from {}", METADATA_API_URL);

        let client = reqwest::Client::new();
        let response = client
            .get(METADATA_API_URL)
            .send()
            .await
            .context("Failed to send metadata request")?;

        if !response.status().is_success() {
            anyhow::bail!("Metadata API returned status: {}", response.status());
        }

        let metadata: MetadataResponse = response
            .json()
            .await
            .context("Failed to parse metadata JSON")?;

        let vendor_count = metadata.len();
        let model_count: usize = metadata.values().map(|v| v.models.len()).sum();

        log::info!(
            "Downloaded metadata: {} vendors, {} models",
            vendor_count,
            model_count
        );

        self.data = Some(metadata);
        Ok(self.data.as_ref().unwrap())
    }

    /// Get cached metadata, downloading if necessary
    pub async fn get(&mut self) -> Result<&MetadataResponse> {
        if self.data.is_none() {
            self.download().await?;
        }
        Ok(self.data.as_ref().unwrap())
    }

    /// Get a specific vendor by ID
    pub async fn get_vendor(&mut self, vendor_id: &str) -> Result<&Vendor> {
        let metadata = self.get().await?;
        metadata
            .get(vendor_id)
            .with_context(|| format!("Vendor '{}' not found", vendor_id))
    }

    /// Get a specific model by vendor and model ID
    pub async fn get_model(&mut self, vendor_id: &str, model_id: &str) -> Result<&Model> {
        let vendor = self.get_vendor(vendor_id).await?;
        vendor
            .models
            .get(model_id)
            .with_context(|| format!("Model '{}' not found for vendor '{}'", model_id, vendor_id))
    }

    /// List all vendors
    pub async fn list_vendors(&mut self) -> Result<Vec<&Vendor>> {
        let metadata = self.get().await?;
        Ok(metadata.values().collect())
    }

    /// List all models for a vendor
    pub async fn list_models(&mut self, vendor_id: &str) -> Result<Vec<&Model>> {
        let vendor = self.get_vendor(vendor_id).await?;
        Ok(vendor.models.values().collect())
    }

    /// Search models by family name
    pub async fn search_by_family(&mut self, family: &str) -> Result<Vec<(&Vendor, &Model)>> {
        let metadata = self.get().await?;
        let results = metadata
            .values()
            .flat_map(|vendor| {
                vendor
                    .models
                    .values()
                    .filter(move |model| model.family.eq_ignore_ascii_case(family))
                    .map(move |model| (vendor, model))
            })
            .collect();
        Ok(results)
    }

    /// Clear cached metadata
    pub fn clear(&mut self) {
        self.data = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vendor_deserialization() {
        let json = r#"{
            "id": "test-vendor",
            "env": ["API_KEY"],
            "npm": "@test/package",
            "api": "https://api.test.com/v1",
            "name": "Test Vendor",
            "doc": "https://docs.test.com",
            "models": {}
        }"#;

        let vendor: Vendor = serde_json::from_str(json).unwrap();
        assert_eq!(vendor.id, "test-vendor");
        assert_eq!(vendor.name, "Test Vendor");
        assert_eq!(vendor.env, vec!["API_KEY"]);
    }

    #[test]
    fn test_model_deserialization() {
        let json = r#"{
            "id": "test-model",
            "name": "Test Model",
            "family": "test",
            "attachment": true,
            "reasoning": false,
            "tool_call": true,
            "structured_output": true,
            "temperature": true,
            "knowledge": "2025-01",
            "release_date": "2025-01-15",
            "last_updated": "2025-01-20",
            "modalities": {
                "input": ["text", "image"],
                "output": ["text"]
            },
            "open_weights": true,
            "cost": {
                "input": 0.5,
                "output": 1.5
            },
            "limit": {
                "context": 128000,
                "output": 8192
            }
        }"#;

        let model: Model = serde_json::from_str(json).unwrap();
        assert_eq!(model.id, "test-model");
        assert_eq!(model.name, "Test Model");
        assert_eq!(model.family, "test");
        assert!(model.attachment);
        assert!(!model.reasoning);
        assert!(model.tool_call);
        assert!(model.structured_output);
        assert!(model.temperature);
        assert_eq!(model.knowledge, Some("2025-01".to_string()));
        assert_eq!(model.release_date, Some("2025-01-15".to_string()));
        assert!(model.modalities.is_some());
        assert!(model.cost.is_some());
        assert!(model.limit.is_some());
    }

    #[test]
    fn test_model_with_optional_fields() {
        let json = r#"{
            "id": "minimal-model",
            "name": "Minimal Model",
            "family": "minimal"
        }"#;

        let model: Model = serde_json::from_str(json).unwrap();
        assert_eq!(model.id, "minimal-model");
        assert_eq!(model.name, "Minimal Model");
        assert!(!model.attachment);
        assert!(!model.reasoning);
        assert!(model.cost.is_none());
    }
}
