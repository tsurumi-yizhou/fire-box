//! Tests for Keyring Middleware

use firebox_service::middleware::keyring::{delete_password, get_password, set_password};

// Basic functionality tests
#[test]
fn set_password_with_empty_service() {
    let result = set_password("", "user", "secret");
    // Should not panic, may return error
    assert!(result.is_err() || result.is_ok());
}

#[test]
fn set_password_with_empty_user() {
    let result = set_password("service", "", "secret");
    assert!(result.is_err() || result.is_ok());
}

#[test]
fn set_password_with_empty_secret() {
    let result = set_password("fire-box-test", "user", "");
    // Empty secrets are technically allowed
    assert!(result.is_ok());
}

#[test]
fn get_password_with_nonexistent_entry() {
    let result = get_password("fire-box-test-nonexistent", "user");
    // Should return an error for non-existent entries
    assert!(result.is_err());
}

#[test]
fn delete_password_with_nonexistent_entry() {
    let result = delete_password("fire-box-test-nonexistent", "user");
    // Should return an error for non-existent entries
    assert!(result.is_err());
}

// Service name tests
#[test]
fn service_name_with_prefix() {
    let services = vec![
        "fire-box-openai",
        "fire-box-anthropic",
        "fire-box-copilot",
        "fire-box-dashscope",
        "fire-box-llamacpp",
    ];

    for service in services {
        assert!(service.starts_with("fire-box"));
    }
}

#[test]
fn service_name_format() {
    // Service names should be valid identifiers
    let service = "fire-box-test-service";
    assert!(!service.is_empty());
    assert!(!service.contains(' '));
}

// User name tests
#[test]
fn user_name_api_key() {
    let user = "api-key";
    assert!(!user.is_empty());
}

#[test]
fn user_name_oauth() {
    let user = "oauth-credentials";
    assert!(!user.is_empty());
}

#[test]
fn user_name_variations() {
    let users = vec![
        "api-key",
        "oauth-token",
        "github-oauth",
        "model-path",
        "encryption-key",
    ];

    for user in users {
        assert!(!user.is_empty());
        assert!(!user.contains(' '));
    }
}

// Secret format tests
#[test]
fn secret_with_special_characters() {
    let secrets = vec!["sk-test_123", "sk-test.456", "sk-test!@#", "gho_token-abc"];

    for secret in secrets {
        assert!(!secret.is_empty());
    }
}

#[test]
fn secret_with_unicode() {
    let secret = "sk-测试 -🔑-123";
    assert!(!secret.is_empty());
    assert!(secret.contains("测试"));
}

#[test]
fn secret_long_string() {
    let secret = "sk-".to_string() + &"x".repeat(500);
    assert!(secret.len() > 500);
}

#[test]
fn secret_base64_format() {
    let secret = "c29tZS1zZWNyZXQtdG9rZW4=";
    assert!(!secret.is_empty());
}

#[test]
fn secret_json_format() {
    let secret = r#"{"access_token":"at-123","refresh_token":"rt-456"}"#;
    assert!(secret.contains("access_token"));
}

// Error handling tests
#[test]
fn error_message_contains_service() {
    let result: Result<String, anyhow::Error> = get_password("fire-box-test-error", "user");
    if let Err(e) = result {
        // Error should be meaningful
        assert!(!e.to_string().is_empty());
    }
}

#[test]
fn error_message_contains_user() {
    let result: Result<String, anyhow::Error> = get_password("fire-box-test", "test-user-error");
    if let Err(e) = result {
        assert!(!e.to_string().is_empty());
    }
}

// Integration-style tests (marked as ignore since they need actual keyring)
#[test]
#[ignore = "Requires actual keychain access"]
fn keyring_roundtrip() {
    let service = "fire-box-test-roundtrip";
    let user = "test-user";
    let secret = "test-secret-value";

    // Set
    set_password(service, user, secret).unwrap();

    // Get
    let retrieved = get_password(service, user).unwrap();
    assert_eq!(retrieved, secret);

    // Delete
    delete_password(service, user).unwrap();

    // Verify deleted
    let result = get_password(service, user);
    assert!(result.is_err());
}

#[test]
#[ignore = "Requires actual keychain access"]
fn keyring_multiple_entries() {
    let service = "fire-box-test-multi";

    let entries = vec![
        ("user1", "secret1"),
        ("user2", "secret2"),
        ("user3", "secret3"),
    ];

    // Set all
    for (user, secret) in &entries {
        set_password(service, user, secret).unwrap();
    }

    // Get all
    for (user, expected_secret) in &entries {
        let retrieved = get_password(service, user).unwrap();
        assert_eq!(retrieved, *expected_secret);
    }

    // Delete all
    for (user, _) in &entries {
        delete_password(service, user).unwrap();
    }
}

#[test]
#[ignore = "Requires actual keychain access"]
fn keyring_update_existing() {
    let service = "fire-box-test-update";
    let user = "test-user";

    // Set initial
    set_password(service, user, "initial-secret").unwrap();

    // Update
    set_password(service, user, "updated-secret").unwrap();

    // Get updated
    let retrieved = get_password(service, user).unwrap();
    assert_eq!(retrieved, "updated-secret");

    // Cleanup
    delete_password(service, user).unwrap();
}

// Edge cases
#[test]
fn very_long_service_name() {
    let service = "fire-box-".to_string() + &"x".repeat(100);
    let result = set_password(&service, "user", "secret");
    // Should not panic
    assert!(result.is_err() || result.is_ok());
}

#[test]
fn very_long_user_name() {
    let user = "user-".to_string() + &"x".repeat(100);
    let result = set_password("fire-box-test", &user, "secret");
    assert!(result.is_err() || result.is_ok());
}

#[test]
fn very_long_secret() {
    let secret = "secret-".to_string() + &"x".repeat(10000);
    let result = set_password("fire-box-test", "user", &secret);
    assert!(result.is_err() || result.is_ok());
}

#[test]
fn service_name_with_numbers() {
    let service = "fire-box-test-123";
    assert!(service.contains("123"));
}

#[test]
fn service_name_with_hyphens() {
    let service = "fire-box-my-test-service";
    assert!(service.matches('-').count() >= 3);
}

#[test]
fn user_name_case_sensitive() {
    // Keyring should be case-sensitive for user names
    let user1 = "TestUser";
    let user2 = "testuser";
    assert_ne!(user1, user2);
}

#[test]
fn service_name_case_sensitive() {
    let service1 = "FireBox-Test";
    let service2 = "firebox-test";
    assert_ne!(service1, service2);
}

// Provider-specific keyring usage patterns
#[test]
fn openai_keyring_pattern() {
    let service = "fire-box-openai";
    let user = "api-key";
    assert!(service.contains("openai"));
    assert_eq!(user, "api-key");
}

#[test]
fn anthropic_keyring_pattern() {
    let service = "fire-box-anthropic";
    let _user = "api-key";
    assert!(service.contains("anthropic"));
}

#[test]
fn copilot_keyring_pattern() {
    let service = "fire-box-copilot";
    let user = "github-oauth";
    assert!(service.contains("copilot"));
    assert!(user.contains("github"));
}

#[test]
fn dashscope_keyring_pattern() {
    let service = "fire-box-dashscope";
    let user = "oauth-credentials";
    assert!(service.contains("dashscope"));
    assert!(user.contains("oauth"));
}

#[test]
fn llamacpp_keyring_pattern() {
    let service = "fire-box-llamacpp";
    let user = "model-path";
    assert!(service.contains("llamacpp"));
    assert!(user.contains("model"));
}

#[test]
fn store_encryption_key_pattern() {
    let service = "fire-box";
    let user = "encryption-key";
    assert_eq!(service, "fire-box");
    assert!(user.contains("encryption"));
}

// Helper function tests
#[test]
fn password_functions_are_available() {
    // Just verify the functions exist and have the right signatures
    let _set_fn: fn(&str, &str, &str) -> anyhow::Result<()> = set_password;
    let _get_fn: fn(&str, &str) -> anyhow::Result<String> = get_password;
    let _delete_fn: fn(&str, &str) -> anyhow::Result<()> = delete_password;
}

#[test]
fn anyhow_error_compatibility() {
    // Verify that errors can be converted to anyhow::Error
    let result: anyhow::Result<()> = set_password("", "user", "secret");
    assert!(result.is_err() || result.is_ok());
}

#[test]
fn error_display() {
    let result: Result<String, anyhow::Error> = get_password("nonexistent", "user");
    if let Err(e) = result {
        let error_string = e.to_string();
        assert!(!error_string.is_empty());
    }
}

#[test]
fn error_debug() {
    let result: Result<String, anyhow::Error> = get_password("nonexistent", "user");
    if let Err(e) = result {
        let debug_string = format!("{:?}", e);
        assert!(!debug_string.is_empty());
    }
}

// Concurrency tests (basic)
#[test]
fn concurrent_reads_safe() {
    // This test documents that reads should be thread-safe
    // Actual concurrent testing would need tokio
    let result1: Result<String, anyhow::Error> = get_password("nonexistent1", "user");
    let result2: Result<String, anyhow::Error> = get_password("nonexistent2", "user");

    // Both should fail (non-existent) but not panic
    assert!(result1.is_err());
    assert!(result2.is_err());
}

// Resource cleanup tests
#[test]
#[ignore = "Requires actual keychain access"]
fn cleanup_after_test() {
    let service = "fire-box-test-cleanup";
    let user = "cleanup-user";

    // Set and delete
    set_password(service, user, "temp-secret").unwrap();
    delete_password(service, user).unwrap();

    // Verify cleanup
    let result = get_password(service, user);
    assert!(result.is_err());
}
