# Route Rules

This page allows administrators to define virtual model routing rules.

## Route Rule Configuration

A list of defined routing rules. Each rule is displayed in a collapsible panel.

### Rule Fields

- **Virtual Model ID:** Unique identifier used by clients (e.g., `gpt-4-coding`).
- **Display Name:** Human-readable name.
- **Capability Requirements:** (e.g., `Chat`, `Streaming`, `Vision`, `Tool Calling`)
    - *Selection via dropdown:* The UI should provide a list of capabilities to choose from.
- **Routing Strategy:**
    - `Failover`: Try targets in sequential order.
    - `Random`: Select a random target from the list.

## Target Configuration

For each rule, configure a list of physical providers and models.

### Target Selection (Dropdown)

- **Provider Selection:** A dropdown showing all configured and enabled providers.
- **Model Selection:** 
    - A dropdown listing all available models for the selected provider.
    - **Capability Filtering:** Models that do not satisfy the rule's capability requirements should be filtered out or clearly marked as incompatible.
    - **User Input:** Manual entry of model IDs is prohibited; selection must be through the dropdown menu.

## Actions

- **Create New Rule:** Button to open the rule creation modal.
- **Edit Rule:** Modify existing rules.
- **Delete Rule:** Remove a routing configuration.

## Implementation Logic

### Model Filtering (Admin UI)

When configuring a route target, the frontend must implement the following filtering logic:

1.  **Capability Check:** A target model is only "Valid" if its `capabilities` (retrieved via `GetAllModels`) are a superset of the rule's `ModelCapabilities` requirements.
2.  **Visual Feedback:** Models that fail the check should be greyed out or moved to an "Incompatible" section in the dropdown, with a tooltip explaining the missing capability.

### Routing Execution (Backend)

The backend routing engine follows these rules:

1.  **Virtual to Physical:** When a client requests a `virtual_model_id`, the backend looks up the assigned `targets`.
2.  **Strategy Application:**
    *   **Failover:** Iterates through targets in the defined order. If a target returns a `PROVIDER_ERROR` or `RATE_LIMITED`, the engine immediately attempts the next target.
    *   **Random:** Selects a target using a uniform-random distribution.
3.  **Circuit Breaking:** If all targets fail, the backend returns the error from the *last* attempted target (or a consolidated `INTERNAL_ERROR`).

## Data Source
Backend's `ListRouteRules`, `GetRouteRules`, and `SetRouteRules` APIs.
