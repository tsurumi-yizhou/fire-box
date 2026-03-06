# Allowlist

This page lists all applications that have been authorized to access the FireBox service.

## Approved Applications List

A table of all applications in the `allowlist`.

### Table Columns

- **Application Name:** Display name of the authorized process.
- **Application Path:** The full filesystem path to the authorized executable.
- **Date Authorized:** When the user first granted access.
- **Last Used:** When the application most recently connected to the service.

## Actions

- **Revoke Access (Remove):** Remove the application from the `allowlist`.
    - If a revoked application attempts to reconnect, the user will be prompted to authorize access again.

## Data Source
Backend's `GetAllowlist` and `RemoveFromAllowlist` APIs.
