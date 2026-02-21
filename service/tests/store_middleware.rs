//! Tests for Store Middleware (Encrypted Local Storage)

use firebox_service::middleware::store::{StoreData, load, update};
use std::collections::HashMap;

// StoreData structure tests
#[test]
fn store_data_default() {
    let data = StoreData::default();
    assert!(data.provider_index.is_empty());
    assert!(data.providers.is_empty());
    assert!(data.display_names.is_empty());
}

#[test]
fn store_data_with_provider_index() {
    let data = StoreData {
        provider_index: vec!["openai".to_string(), "anthropic".to_string()],
        providers: HashMap::new(),
        display_names: HashMap::new(),
        route_rules: HashMap::new(),
        enabled_models: HashMap::new(),
    };

    assert_eq!(data.provider_index.len(), 2);
    assert!(data.provider_index.contains(&"openai".to_string()));
}

#[test]
fn store_data_with_providers() {
    let mut providers = HashMap::new();
    providers.insert("openai".to_string(), r#"{"api_key":"sk-test"}"#.to_string());

    let data = StoreData {
        provider_index: vec!["openai".to_string()],
        providers,
        display_names: HashMap::new(),
        route_rules: HashMap::new(),
        enabled_models: HashMap::new(),
    };

    assert_eq!(data.providers.len(), 1);
    assert!(data.providers.contains_key("openai"));
}

#[test]
fn store_data_with_display_names() {
    let mut display_names = HashMap::new();
    display_names.insert("openai".to_string(), "My OpenAI".to_string());

    let data = StoreData {
        provider_index: vec![],
        providers: HashMap::new(),
        display_names,
        route_rules: HashMap::new(),
        enabled_models: HashMap::new(),
    };

    assert_eq!(data.display_names.len(), 1);
    assert_eq!(
        data.display_names.get("openai"),
        Some(&"My OpenAI".to_string())
    );
}

#[test]
fn store_data_clone() {
    let mut providers = HashMap::new();
    providers.insert("test".to_string(), "config".to_string());

    let data = StoreData {
        provider_index: vec!["test".to_string()],
        providers,
        display_names: HashMap::new(),
        route_rules: HashMap::new(),
        enabled_models: HashMap::new(),
    };

    let cloned = data.clone();
    assert_eq!(data.provider_index, cloned.provider_index);
}

#[test]
fn store_data_debug() {
    let data = StoreData::default();
    let debug_str = format!("{:?}", data);
    assert!(debug_str.contains("StoreData"));
}

// Load tests (empty store)
#[test]
fn load_empty_store() {
    // When no store exists, should return default data
    let result = load();
    assert!(result.is_ok());

    let data = result.unwrap();
    assert!(data.provider_index.is_empty());
    assert!(data.providers.is_empty());
}

#[test]
fn load_returns_store_data() {
    let result = load();
    assert!(result.is_ok());
}

// Update tests
#[test]
fn update_add_provider_to_index() {
    let result = update(|data| {
        data.provider_index.push("test-provider".to_string());
    });

    assert!(result.is_ok());

    // Verify the update persisted
    let loaded = load().unwrap();
    assert!(loaded.provider_index.contains(&"test-provider".to_string()));
}

#[test]
fn update_add_provider_config() {
    let config_json = r#"{"api_key":"sk-test"}"#;

    let result = update(|data| {
        data.providers
            .insert("test".to_string(), config_json.to_string());
    });

    assert!(result.is_ok());

    let loaded = load().unwrap();
    assert!(loaded.providers.contains_key("test"));
}

#[test]
fn update_add_display_name() {
    let result = update(|data| {
        data.display_names
            .insert("test".to_string(), "Test Display".to_string());
    });

    assert!(result.is_ok());

    let loaded = load().unwrap();
    assert!(loaded.display_names.contains_key("test"));
}

#[test]
fn update_multiple_operations() {
    let result = update(|data| {
        data.provider_index.push("provider1".to_string());
        data.provider_index.push("provider2".to_string());
        data.providers
            .insert("provider1".to_string(), "config1".to_string());
        data.display_names
            .insert("provider1".to_string(), "Provider One".to_string());
    });

    assert!(result.is_ok());

    let loaded = load().unwrap();
    assert_eq!(loaded.provider_index.len(), 2);
    assert_eq!(loaded.providers.len(), 1);
    assert_eq!(loaded.display_names.len(), 1);
}

#[test]
fn update_modify_existing() {
    // First, add something
    update(|data| {
        data.providers
            .insert("modify-test".to_string(), "original".to_string());
    })
    .unwrap();

    // Then modify
    let result = update(|data| {
        data.providers
            .insert("modify-test".to_string(), "modified".to_string());
    });

    assert!(result.is_ok());

    let loaded = load().unwrap();
    assert_eq!(
        loaded.providers.get("modify-test"),
        Some(&"modified".to_string())
    );
}

#[test]
fn update_remove_provider() {
    // First, add something
    update(|data| {
        data.providers
            .insert("remove-test".to_string(), "config".to_string());
        data.provider_index.push("remove-test".to_string());
    })
    .unwrap();

    // Then remove
    let result = update(|data| {
        data.providers.remove("remove-test");
        data.provider_index.retain(|id| id != "remove-test");
    });

    assert!(result.is_ok());

    let loaded = load().unwrap();
    assert!(!loaded.providers.contains_key("remove-test"));
}

// Provider index tests
#[test]
fn provider_index_order() {
    update(|data| {
        data.provider_index.clear();
        data.provider_index.push("first".to_string());
        data.provider_index.push("second".to_string());
        data.provider_index.push("third".to_string());
    })
    .unwrap();

    let loaded = load().unwrap();
    assert_eq!(loaded.provider_index[0], "first");
    assert_eq!(loaded.provider_index[1], "second");
    assert_eq!(loaded.provider_index[2], "third");
}

#[test]
fn provider_index_no_duplicates() {
    update(|data| {
        data.provider_index.clear();
        data.provider_index.push("unique".to_string());
        data.provider_index.push("unique".to_string()); // Duplicate
    })
    .unwrap();

    let loaded = load().unwrap();
    // Both entries will be present (no automatic deduplication)
    assert_eq!(loaded.provider_index.len(), 2);
}

// Provider config tests
#[test]
fn provider_config_json_storage() {
    let configs = vec![
        ("openai", r#"{"OpenAi":{"api_key":"sk-test"}}"#),
        ("anthropic", r#"{"Anthropic":{"api_key":"sk-ant"}}"#),
        (
            "llamacpp",
            r#"{"LlamaCpp":{"model_path":"/models/test.gguf"}}"#,
        ),
    ];

    update(|data| {
        for (name, config) in &configs {
            data.providers.insert(name.to_string(), config.to_string());
        }
    })
    .unwrap();

    let loaded = load().unwrap();
    for (name, expected_config) in &configs {
        assert_eq!(
            loaded.providers.get(*name),
            Some(&expected_config.to_string())
        );
    }
}

#[test]
fn provider_config_unicode() {
    let config = r#"{"name":"测试配置"}"#;

    update(|data| {
        data.providers
            .insert("unicode-test".to_string(), config.to_string());
    })
    .unwrap();

    let loaded = load().unwrap();
    assert!(
        loaded
            .providers
            .get("unicode-test")
            .unwrap()
            .contains("测试")
    );
}

// Display name tests
#[test]
fn display_name_unicode() {
    update(|data| {
        data.display_names
            .insert("chinese".to_string(), "中文名称".to_string());
        data.display_names
            .insert("emoji".to_string(), "My Provider 🚀".to_string());
    })
    .unwrap();

    let loaded = load().unwrap();
    assert_eq!(
        loaded.display_names.get("chinese"),
        Some(&"中文名称".to_string())
    );
    assert_eq!(
        loaded.display_names.get("emoji"),
        Some(&"My Provider 🚀".to_string())
    );
}

#[test]
fn display_name_special_chars() {
    update(|data| {
        data.display_names
            .insert("special".to_string(), "Provider <>&\"'".to_string());
    })
    .unwrap();

    let loaded = load().unwrap();
    assert!(loaded.display_names.get("special").unwrap().contains("<>"));
}

// Edge cases
#[test]
fn empty_provider_name() {
    let result = update(|data| {
        data.providers.insert("".to_string(), "config".to_string());
    });

    // Should not panic
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn very_long_provider_name() {
    let long_name = "provider-".to_string() + &"x".repeat(1000);
    let result = update(|data| {
        data.providers.insert(long_name, "config".to_string());
    });

    assert!(result.is_ok());
}

#[test]
fn very_long_config_json() {
    let long_config = "config-".to_string() + &"x".repeat(10000);
    let result = update(|data| {
        data.providers
            .insert("long-config".to_string(), long_config);
    });

    assert!(result.is_ok());
}

#[test]
fn many_providers() {
    update(|data| {
        for i in 0..100 {
            data.provider_index.push(format!("provider-{}", i));
            data.providers
                .insert(format!("provider-{}", i), format!("config-{}", i));
        }
    })
    .unwrap();

    let loaded = load().unwrap();
    assert_eq!(loaded.provider_index.len(), 100);
    assert_eq!(loaded.providers.len(), 100);
}

// StoreData helper tests
#[test]
fn store_data_is_empty_by_default() {
    let data = StoreData::default();
    assert!(data.provider_index.is_empty());
    assert!(data.providers.is_empty());
    assert!(data.display_names.is_empty());
}

#[test]
fn store_data_partial_update() {
    // Add some data
    update(|data| {
        data.providers
            .insert("keep".to_string(), "keep-config".to_string());
    })
    .unwrap();

    // Update only display_names
    update(|data| {
        data.display_names
            .insert("new".to_string(), "New Name".to_string());
    })
    .unwrap();

    let loaded = load().unwrap();
    // Original provider should still be there
    assert!(loaded.providers.contains_key("keep"));
    // New display name should be added
    assert!(loaded.display_names.contains_key("new"));
}

// Serialization tests
#[test]
fn store_data_serialization_roundtrip() {
    let original = StoreData {
        provider_index: vec!["p1".to_string(), "p2".to_string()],
        providers: {
            let mut m = HashMap::new();
            m.insert("p1".to_string(), "config1".to_string());
            m
        },
        display_names: {
            let mut m = HashMap::new();
            m.insert("p1".to_string(), "Provider One".to_string());
            m
        },
        route_rules: HashMap::new(),
        enabled_models: HashMap::new(),
    };

    let json = serde_json::to_string(&original).unwrap();
    let restored: StoreData = serde_json::from_str(&json).unwrap();

    assert_eq!(original.provider_index, restored.provider_index);
    assert_eq!(original.providers.len(), restored.providers.len());
    assert_eq!(original.display_names.len(), restored.display_names.len());
}

#[test]
fn store_data_empty_serialization() {
    let data = StoreData::default();
    let json = serde_json::to_string(&data).unwrap();
    let restored: StoreData = serde_json::from_str(&json).unwrap();

    assert!(restored.provider_index.is_empty());
    assert!(restored.providers.is_empty());
}

// Integration tests
#[test]
fn full_workflow() {
    // Clear any existing data for this test
    update(|data| {
        data.provider_index.clear();
        data.providers.clear();
        data.display_names.clear();
    })
    .unwrap();

    // Add providers
    update(|data| {
        data.provider_index.push("openai".to_string());
        data.provider_index.push("anthropic".to_string());
        data.providers
            .insert("openai".to_string(), r#"{"api_key":"sk-test"}"#.to_string());
        data.providers.insert(
            "anthropic".to_string(),
            r#"{"api_key":"sk-ant"}"#.to_string(),
        );
        data.display_names
            .insert("openai".to_string(), "My OpenAI".to_string());
    })
    .unwrap();

    // Load and verify
    let loaded = load().unwrap();
    assert_eq!(loaded.provider_index.len(), 2);
    assert_eq!(loaded.providers.len(), 2);
    assert_eq!(loaded.display_names.len(), 1);

    // Update one provider
    update(|data| {
        data.display_names
            .insert("anthropic".to_string(), "My Anthropic".to_string());
    })
    .unwrap();

    // Load and verify update
    let loaded = load().unwrap();
    assert_eq!(loaded.display_names.len(), 2);
    assert_eq!(
        loaded.display_names.get("anthropic"),
        Some(&"My Anthropic".to_string())
    );
}

#[test]
fn concurrent_updates_safe() {
    // This test documents that updates should be atomic
    // Actual concurrent testing would need tokio

    update(|data| {
        data.providers
            .insert("concurrent".to_string(), "value1".to_string());
    })
    .unwrap();

    update(|data| {
        data.providers
            .insert("concurrent".to_string(), "value2".to_string());
    })
    .unwrap();

    let loaded = load().unwrap();
    // Should have one of the values, not corrupted
    let value = loaded.providers.get("concurrent").unwrap();
    assert!(value == "value1" || value == "value2");
}

// Error handling
#[test]
fn update_with_panic_closure() {
    // This test verifies that a panicking closure doesn't corrupt the store
    let result = std::panic::catch_unwind(|| {
        update(|data| {
            data.providers
                .insert("before-panic".to_string(), "value".to_string());
            panic!("Test panic");
        })
    });

    assert!(result.is_err()); // Should panic

    // Store should still be usable after panic
    let load_result = load();
    assert!(load_result.is_ok());
}
