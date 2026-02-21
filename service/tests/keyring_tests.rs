//! Integration tests for keyring storage.

use firebox_service::middleware::keyring;

#[test]
#[ignore] // Requires actual keychain access
fn test_keyring_set_and_get() {
    let service = "fire-box-test";
    let user = "test-user";
    let secret = "test-secret-value";

    // Set password
    let result = keyring::set_password(service, user, secret);
    assert!(result.is_ok(), "Should set password successfully");

    // Get password
    let result = keyring::get_password(service, user);
    assert!(result.is_ok(), "Should get password successfully");
    assert_eq!(result.unwrap(), secret, "Retrieved password should match");

    // Clean up
    let _ = keyring::delete_password(service, user);
}

#[test]
#[ignore] // Requires actual keychain access
fn test_keyring_delete() {
    let service = "fire-box-test";
    let user = "test-delete-user";
    let secret = "test-secret";

    // Set password
    keyring::set_password(service, user, secret).unwrap();

    // Delete password
    let result = keyring::delete_password(service, user);
    assert!(result.is_ok(), "Should delete password successfully");

    // Verify it's gone
    let result = keyring::get_password(service, user);
    assert!(result.is_err(), "Should not find deleted password");
}

#[test]
fn test_keyring_get_nonexistent() {
    let service = "fire-box-test";
    let user = "nonexistent-user";

    let result = keyring::get_password(service, user);
    assert!(result.is_err(), "Should fail to get nonexistent password");
}

#[test]
#[ignore] // Requires actual keychain access
fn test_keyring_update() {
    let service = "fire-box-test";
    let user = "test-update-user";
    let secret1 = "first-secret";
    let secret2 = "second-secret";

    // Set initial password
    keyring::set_password(service, user, secret1).unwrap();

    // Update password
    keyring::set_password(service, user, secret2).unwrap();

    // Verify updated value
    let result = keyring::get_password(service, user).unwrap();
    assert_eq!(result, secret2, "Should retrieve updated password");

    // Clean up
    let _ = keyring::delete_password(service, user);
}

#[test]
#[ignore] // Requires actual keychain access
fn test_keyring_multiple_services() {
    let service1 = "fire-box-test-1";
    let service2 = "fire-box-test-2";
    let user = "test-user";
    let secret1 = "secret-1";
    let secret2 = "secret-2";

    // Set passwords in different services
    keyring::set_password(service1, user, secret1).unwrap();
    keyring::set_password(service2, user, secret2).unwrap();

    // Verify both are stored independently
    assert_eq!(keyring::get_password(service1, user).unwrap(), secret1);
    assert_eq!(keyring::get_password(service2, user).unwrap(), secret2);

    // Clean up
    let _ = keyring::delete_password(service1, user);
    let _ = keyring::delete_password(service2, user);
}
