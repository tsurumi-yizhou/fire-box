#pragma once
/// @file credential.hpp
/// Secure credential storage using systemd-creds encrypt/decrypt.

#include <string>

namespace firebox {

/// Store a secret (API key, OAuth token, etc.) encrypted via systemd-creds.
/// @param name  logical name (e.g. "provider-openai-apikey")
/// @param value the plaintext secret
/// @return true on success
bool credential_store(const std::string& name, const std::string& value);

/// Retrieve a previously stored secret.
/// @param name logical name
/// @return the decrypted plaintext, or empty string on failure
std::string credential_load(const std::string& name);

/// Delete a stored credential.
bool credential_delete(const std::string& name);

} // namespace firebox
