//! Integration tests for encrypted store.

use firebox_service::middleware::store;

#[test]
#[ignore] // Requires keychain access
fn test_store_load_empty() {
    let data = store::load();
    assert!(data.is_ok(), "Should load empty store successfully");
    let data = data.unwrap();
    assert!(data.provider_index.is_empty());
    assert!(data.providers.is_empty());
}

#[test]
#[ignore] // Requires keychain access
fn test_store_update_and_load() {
    // Update store
    let result = store::update(|data| {
        data.provider_index.push("test-provider".to_string());
        data.providers.insert(
            "test-provider".to_string(),
            r#"{"type":"openai","api_key":"test"}"#.to_string(),
        );
        data.display_names
            .insert("test-provider".to_string(), "Test Provider".to_string());
    });
    assert!(result.is_ok(), "Should update store successfully");

    // Load and verify
    let data = store::load().unwrap();
    assert_eq!(data.provider_index.len(), 1);
    assert_eq!(data.provider_index[0], "test-provider");
    assert!(data.providers.contains_key("test-provider"));
    assert_eq!(
        data.display_names.get("test-provider").unwrap(),
        "Test Provider"
    );

    // Clean up
    let _ = store::update(|data| {
        data.provider_index.clear();
        data.providers.clear();
        data.display_names.clear();
    });
}

#[test]
#[ignore] // Requires keychain access
fn test_store_multiple_providers() {
    let result = store::update(|data| {
        data.provider_index = vec![
            "provider-1".to_string(),
            "provider-2".to_string(),
            "provider-3".to_string(),
        ];
        data.providers
            .insert("provider-1".to_string(), "{}".to_string());
        data.providers
            .insert("provider-2".to_string(), "{}".to_string());
        data.providers
            .insert("provider-3".to_string(), "{}".to_string());
    });
    assert!(result.is_ok());

    let data = store::load().unwrap();
    assert_eq!(data.provider_index.len(), 3);
    assert_eq!(data.providers.len(), 3);

    // Clean up
    let _ = store::update(|data| {
        data.provider_index.clear();
        data.providers.clear();
    });
}

#[test]
#[ignore] // Requires keychain access
fn test_store_route_rules() {
    let result = store::update(|data| {
        data.route_rules.insert(
            "default".to_string(),
            r#"{"provider":"openai","model":"gpt-4"}"#.to_string(),
        );
    });
    assert!(result.is_ok());

    let data = store::load().unwrap();
    assert!(data.route_rules.contains_key("default"));

    // Clean up
    let _ = store::update(|data| {
        data.route_rules.clear();
    });
}

#[test]
#[ignore] // Requires keychain access
fn test_store_enabled_models() {
    let result = store::update(|data| {
        data.enabled_models.insert(
            "openai".to_string(),
            vec!["gpt-4".to_string(), "gpt-3.5-turbo".to_string()],
        );
    });
    assert!(result.is_ok());

    let data = store::load().unwrap();
    assert!(data.enabled_models.contains_key("openai"));
    assert_eq!(data.enabled_models.get("openai").unwrap().len(), 2);

    // Clean up
    let _ = store::update(|data| {
        data.enabled_models.clear();
    });
}

#[test]
#[ignore] // Requires keychain access
fn test_store_persistence() {
    // First update
    store::update(|data| {
        data.provider_index.push("persistent-provider".to_string());
    })
    .unwrap();

    // Load in separate call
    let data1 = store::load().unwrap();
    assert_eq!(data1.provider_index.len(), 1);

    // Second update
    store::update(|data| {
        data.provider_index.push("another-provider".to_string());
    })
    .unwrap();

    // Verify both are present
    let data2 = store::load().unwrap();
    assert_eq!(data2.provider_index.len(), 2);

    // Clean up
    let _ = store::update(|data| {
        data.provider_index.clear();
    });
}
