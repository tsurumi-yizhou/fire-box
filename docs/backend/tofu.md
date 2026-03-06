# TOFU Authorization

The FireBox backend implements user-facing authorization logic to handle unknown applications on first connection. When an unauthorized application attempts to connect, the user must explicitly grant or deny access.

## Authorization UI

When a new application attempts to connect, the backend initiates an authorization interaction to inform the user and capture their decision.

### User Interaction

The authorization flow presents the user with:

- **Application Information:** Identity of the calling application (name, path, or other identifying details).
- **Request Context:** Clear indication that the application is requesting access to FireBox capabilities.
- **User Choice:** An explicit decision point:
    - **Grant:** Adds the application to the `allowlist` for future connections.
    - **Deny:** Rejects the connection request.

The mechanism for presenting this information and capturing the user's decision is implementation-defined and may vary by platform.
