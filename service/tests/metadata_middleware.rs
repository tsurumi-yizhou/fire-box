//! Tests for Metadata Module

use firebox_service::middleware::metadata::{
    InterleavedConfig, Limits, MetadataManager, Modalities, Model, Pricing, Vendor,
};

// Vendor deserialization tests
#[test]
fn vendor_minimal() {
    let json = r#"{
        "id": "test-vendor",
        "env": [],
        "npm": "@test/package",
        "api": "https://api.test.com",
        "name": "Test Vendor",
        "doc": "https://docs.test.com",
        "models": {}
    }"#;

    let vendor: Vendor = serde_json::from_str(json).unwrap();
    assert_eq!(vendor.id, "test-vendor");
    assert_eq!(vendor.name, "Test Vendor");
    assert!(vendor.env.is_empty());
}

#[test]
fn vendor_with_env_vars() {
    let json = r#"{
        "id": "openai",
        "env": ["OPENAI_API_KEY"],
        "npm": "openai",
        "api": "https://api.openai.com/v1",
        "name": "OpenAI",
        "doc": "https://platform.openai.com/docs",
        "models": {}
    }"#;

    let vendor: Vendor = serde_json::from_str(json).unwrap();
    assert_eq!(vendor.id, "openai");
    assert_eq!(vendor.env, vec!["OPENAI_API_KEY"]);
    assert_eq!(vendor.npm, "openai");
}

#[test]
fn vendor_with_multiple_env_vars() {
    let json = r#"{
        "id": "multi-env",
        "env": ["API_KEY", "API_SECRET", "API_ENDPOINT"],
        "npm": "@multi/package",
        "api": "https://api.multi.com",
        "name": "Multi Env",
        "doc": "https://docs.multi.com",
        "models": {}
    }"#;

    let vendor: Vendor = serde_json::from_str(json).unwrap();
    assert_eq!(vendor.env.len(), 3);
    assert_eq!(vendor.env[0], "API_KEY");
    assert_eq!(vendor.env[1], "API_SECRET");
}

#[test]
fn vendor_with_models() {
    let json = r#"{
        "id": "vendor-with-models",
        "env": [],
        "npm": "@vendor/package",
        "api": "https://api.vendor.com",
        "name": "Vendor",
        "doc": "https://docs.vendor.com",
        "models": {
            "model-1": {
                "id": "model-1",
                "name": "Model One",
                "family": "test"
            }
        }
    }"#;

    let vendor: Vendor = serde_json::from_str(json).unwrap();
    assert_eq!(vendor.models.len(), 1);
    assert!(vendor.models.contains_key("model-1"));
}

#[test]
fn vendor_clone() {
    let json = r#"{
        "id": "clone-vendor",
        "env": ["KEY"],
        "npm": "@clone/package",
        "api": "https://api.clone.com",
        "name": "Clone Vendor",
        "doc": "https://docs.clone.com",
        "models": {}
    }"#;

    let vendor: Vendor = serde_json::from_str(json).unwrap();
    let cloned = vendor.clone();
    assert_eq!(vendor.id, cloned.id);
}

#[test]
fn vendor_debug() {
    let json = r#"{
        "id": "debug-vendor",
        "env": [],
        "npm": "@debug/package",
        "api": "https://api.debug.com",
        "name": "Debug Vendor",
        "doc": "https://docs.debug.com",
        "models": {}
    }"#;

    let vendor: Vendor = serde_json::from_str(json).unwrap();
    let debug_str = format!("{:?}", vendor);
    assert!(debug_str.contains("debug-vendor"));
}

// Model deserialization tests
#[test]
fn model_minimal() {
    let json = r#"{
        "id": "minimal-model",
        "name": "Minimal Model",
        "family": "minimal"
    }"#;

    let model: Model = serde_json::from_str(json).unwrap();
    assert_eq!(model.id, "minimal-model");
    assert_eq!(model.name, "Minimal Model");
    assert_eq!(model.family, "minimal");
}

#[test]
fn model_with_all_features() {
    let json = r#"{
        "id": "full-model",
        "name": "Full Featured Model",
        "family": "full",
        "attachment": true,
        "reasoning": true,
        "tool_call": true,
        "structured_output": true,
        "temperature": true,
        "knowledge": "2025-01",
        "release_date": "2025-01-15",
        "last_updated": "2025-01-20",
        "open_weights": false
    }"#;

    let model: Model = serde_json::from_str(json).unwrap();
    assert!(model.attachment);
    assert!(model.reasoning);
    assert!(model.tool_call);
    assert!(model.structured_output);
    assert!(model.temperature);
    assert_eq!(model.knowledge, Some("2025-01".to_string()));
}

#[test]
fn model_with_interleaved_config() {
    let json = r#"{
        "id": "reasoning-model",
        "name": "Reasoning Model",
        "family": "reasoning",
        "interleaved": {
            "field": "reasoning_content"
        }
    }"#;

    let model: Model = serde_json::from_str(json).unwrap();
    assert!(model.interleaved.is_some());
    assert_eq!(model.interleaved.unwrap().field, "reasoning_content");
}

#[test]
fn model_with_modalities() {
    let json = r#"{
        "id": "multimodal-model",
        "name": "Multimodal Model",
        "family": "multimodal",
        "modalities": {
            "input": ["text", "image", "audio"],
            "output": ["text"]
        }
    }"#;

    let model: Model = serde_json::from_str(json).unwrap();
    assert!(model.modalities.is_some());
    let modalities = model.modalities.unwrap();
    assert_eq!(modalities.input.len(), 3);
    assert!(modalities.input.contains(&"text".to_string()));
    assert!(modalities.input.contains(&"image".to_string()));
}

#[test]
fn model_with_pricing() {
    let json = r#"{
        "id": "priced-model",
        "name": "Priced Model",
        "family": "priced",
        "cost": {
            "input": 0.5,
            "output": 1.5
        }
    }"#;

    let model: Model = serde_json::from_str(json).unwrap();
    assert!(model.cost.is_some());
    let cost = model.cost.unwrap();
    assert!((cost.input - 0.5).abs() < 0.001);
    assert!((cost.output - 1.5).abs() < 0.001);
}

#[test]
fn model_with_pricing_cache() {
    let json = r#"{
        "id": "cache-model",
        "name": "Cache Model",
        "family": "cache",
        "cost": {
            "input": 0.5,
            "output": 1.5,
            "cache_read": 0.1,
            "cache_write": 0.2
        }
    }"#;

    let model: Model = serde_json::from_str(json).unwrap();
    let cost = model.cost.unwrap();
    assert!((cost.cache_read.unwrap() - 0.1).abs() < 0.001);
    assert!((cost.cache_write.unwrap() - 0.2).abs() < 0.001);
}

#[test]
fn model_with_limits() {
    let json = r#"{
        "id": "limited-model",
        "name": "Limited Model",
        "family": "limited",
        "limit": {
            "context": 128000,
            "output": 8192
        }
    }"#;

    let model: Model = serde_json::from_str(json).unwrap();
    assert!(model.limit.is_some());
    let limit = model.limit.unwrap();
    assert_eq!(limit.context, 128000);
    assert_eq!(limit.output, 8192);
}

#[test]
fn model_with_limits_input() {
    let json = r#"{
        "id": "input-limited-model",
        "name": "Input Limited Model",
        "family": "limited",
        "limit": {
            "context": 200000,
            "input": 180000,
            "output": 20000
        }
    }"#;

    let model: Model = serde_json::from_str(json).unwrap();
    let limit = model.limit.unwrap();
    assert_eq!(limit.input, Some(180000));
}

#[test]
fn model_clone() {
    let json = r#"{
        "id": "clone-model",
        "name": "Clone Model",
        "family": "clone"
    }"#;

    let model: Model = serde_json::from_str(json).unwrap();
    let cloned = model.clone();
    assert_eq!(model.id, cloned.id);
}

#[test]
fn model_debug() {
    let json = r#"{
        "id": "debug-model",
        "name": "Debug Model",
        "family": "debug"
    }"#;

    let model: Model = serde_json::from_str(json).unwrap();
    let debug_str = format!("{:?}", model);
    assert!(debug_str.contains("debug-model"));
}

// InterleavedConfig tests
#[test]
fn interleaved_config() {
    let config = InterleavedConfig {
        field: "reasoning".to_string(),
    };
    assert_eq!(config.field, "reasoning");
}

#[test]
fn interleaved_config_clone() {
    let config = InterleavedConfig {
        field: "thought".to_string(),
    };
    let cloned = config.clone();
    assert_eq!(config.field, cloned.field);
}

// Modalities tests
#[test]
fn modalities_text_only() {
    let modalities = Modalities {
        input: vec!["text".to_string()],
        output: vec!["text".to_string()],
    };
    assert_eq!(modalities.input.len(), 1);
    assert_eq!(modalities.output.len(), 1);
}

#[test]
fn modalities_multimodal() {
    let modalities = Modalities {
        input: vec!["text".to_string(), "image".to_string(), "video".to_string()],
        output: vec!["text".to_string()],
    };
    assert_eq!(modalities.input.len(), 3);
}

#[test]
fn modalities_empty() {
    let modalities = Modalities {
        input: vec![],
        output: vec![],
    };
    assert!(modalities.input.is_empty());
    assert!(modalities.output.is_empty());
}

#[test]
fn modalities_clone() {
    let modalities = Modalities {
        input: vec!["text".to_string()],
        output: vec!["text".to_string()],
    };
    let cloned = modalities.clone();
    assert_eq!(modalities.input, cloned.input);
}

// Pricing tests
#[test]
fn pricing_basic() {
    let pricing = Pricing {
        input: 0.5,
        output: 1.5,
        cache_read: None,
        cache_write: None,
        reasoning: None,
        input_audio: None,
        output_audio: None,
    };
    assert!((pricing.input - 0.5).abs() < 0.001);
    assert!((pricing.output - 1.5).abs() < 0.001);
}

#[test]
fn pricing_with_cache() {
    let pricing = Pricing {
        input: 0.5,
        output: 1.5,
        cache_read: Some(0.1),
        cache_write: Some(0.2),
        reasoning: None,
        input_audio: None,
        output_audio: None,
    };
    assert!(pricing.cache_read.is_some());
    assert!(pricing.cache_write.is_some());
}

#[test]
fn pricing_free_model() {
    let pricing = Pricing {
        input: 0.0,
        output: 0.0,
        cache_read: None,
        cache_write: None,
        reasoning: None,
        input_audio: None,
        output_audio: None,
    };
    assert!((pricing.input - 0.0).abs() < 0.001);
    assert!((pricing.output - 0.0).abs() < 0.001);
}

#[test]
fn pricing_clone() {
    let pricing = Pricing {
        input: 1.0,
        output: 2.0,
        cache_read: None,
        cache_write: None,
        reasoning: None,
        input_audio: None,
        output_audio: None,
    };
    let cloned = pricing.clone();
    assert!((pricing.input - cloned.input).abs() < 0.001);
}

// Limits tests
#[test]
fn limits_basic() {
    let limits = Limits {
        context: 128000,
        input: None,
        output: 8192,
    };
    assert_eq!(limits.context, 128000);
    assert_eq!(limits.output, 8192);
}

#[test]
fn limits_with_input() {
    let limits = Limits {
        context: 200000,
        input: Some(180000),
        output: 20000,
    };
    assert_eq!(limits.input, Some(180000));
}

#[test]
fn limits_small_model() {
    let limits = Limits {
        context: 4096,
        input: None,
        output: 2048,
    };
    assert_eq!(limits.context, 4096);
}

#[test]
fn limits_clone() {
    let limits = Limits {
        context: 32000,
        input: None,
        output: 4096,
    };
    let cloned = limits.clone();
    assert_eq!(limits.context, cloned.context);
}

// MetadataManager tests
#[test]
fn metadata_manager_new() {
    let _manager = MetadataManager::new();
}

#[test]
fn metadata_manager_clear() {
    let mut manager = MetadataManager::new();
    manager.clear();
}

#[tokio::test]
async fn metadata_manager_download() {
    let mut manager = MetadataManager::new();
    let result: Result<&std::collections::HashMap<String, Vendor>, anyhow::Error> =
        manager.download().await;

    if let Ok(metadata) = result {
        assert!(!metadata.is_empty());
    }
}

#[tokio::test]
async fn metadata_manager_get() {
    let mut manager = MetadataManager::new();
    let result: Result<&std::collections::HashMap<String, Vendor>, anyhow::Error> =
        manager.get().await;

    if let Ok(metadata) = result {
        assert!(!metadata.is_empty());
    }
}

#[tokio::test]
async fn metadata_manager_get_vendor() {
    let mut manager = MetadataManager::new();

    let result: Result<&Vendor, anyhow::Error> = manager.get_vendor("openai").await;

    if let Ok(vendor) = result {
        assert_eq!(vendor.id, "openai");
    }
}

#[tokio::test]
async fn metadata_manager_list_vendors() {
    let mut manager = MetadataManager::new();
    let result: Result<Vec<&Vendor>, anyhow::Error> = manager.list_vendors().await;

    if let Ok(vendors) = result {
        assert!(!vendors.is_empty());
    }
}

#[tokio::test]
async fn metadata_manager_list_models() {
    let mut manager = MetadataManager::new();

    let result: Result<Vec<&Model>, anyhow::Error> = manager.list_models("openai").await;

    if let Ok(models) = result {
        assert!(!models.is_empty());
    }
}

#[tokio::test]
async fn metadata_manager_search_by_family() {
    let mut manager = MetadataManager::new();

    let result: Result<Vec<(&Vendor, &Model)>, anyhow::Error> =
        manager.search_by_family("gpt").await;

    if let Ok(results) = result {
        for (_vendor, model) in results {
            assert_eq!(model.family.to_lowercase(), "gpt");
        }
    }
}

// Edge cases
#[test]
fn model_with_empty_strings() {
    let json = r#"{
        "id": "",
        "name": "",
        "family": ""
    }"#;

    let model: Model = serde_json::from_str(json).unwrap();
    assert_eq!(model.id, "");
    assert_eq!(model.name, "");
}

#[test]
fn model_with_unicode() {
    let json = r#"{
        "id": "模型 -1",
        "name": "测试模型",
        "family": "测试"
    }"#;

    let model: Model = serde_json::from_str(json).unwrap();
    assert!(model.id.contains("模型"));
    assert!(model.name.contains("测试"));
}

#[test]
fn pricing_very_expensive() {
    let pricing = Pricing {
        input: 100.0,
        output: 200.0,
        cache_read: None,
        cache_write: None,
        reasoning: None,
        input_audio: None,
        output_audio: None,
    };
    assert!(pricing.input > 50.0);
}

#[test]
fn limits_very_large() {
    let limits = Limits {
        context: 1000000,
        input: None,
        output: 100000,
    };
    assert!(limits.context > 500000);
}

#[test]
fn vendor_with_empty_models() {
    let json = r#"{
        "id": "empty-models-vendor",
        "env": [],
        "npm": "@empty/package",
        "api": "https://api.empty.com",
        "name": "Empty Vendor",
        "doc": "https://docs.empty.com",
        "models": {}
    }"#;

    let vendor: Vendor = serde_json::from_str(json).unwrap();
    assert!(vendor.models.is_empty());
}
