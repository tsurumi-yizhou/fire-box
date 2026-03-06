# Settings

The settings page manages provider configurations and general application information.

## Provider Configuration

A grid of provider cards. Each card displays the provider's details and provides management actions.

### Provider Card Components

- **Provider Name:** (e.g., "OpenAI", "GitHub Copilot")
- **Status Indicator:** (e.g., "Enabled", "Disabled", "Authentication Required")
- **Type Badge:** (e.g., "API Key", "OAuth")
- **Model Button:** Opens the **Model Visibility Dialog**.
- **Edit Button:** Opens the provider's **Edit Provider Modal**.
- **Delete Button:** Removes the provider from the service.

### Model Visibility Dialog

- **Search Box:** Filter models by name.
- **Model List:** Toggle switch for each model to enable/disable its availability in the service.

### Add/Edit Provider Modal

#### API Key Providers
- **Name:** (e.g., "My OpenAI Account")
- **Base URL:** The API endpoint.
- **API Key:** The authentication token.
- **Protocol Type (Dropdown):** Choose between `OpenAI`, `Anthropic`, or `Gemini`.

#### OAuth Providers (e.g., GitHub Copilot, DashScope)
- **Name:** (e.g., "GitHub Copilot (Personal)")
- **Authentication Flow:**
    - Button to "Start Authentication".
    - Display **Device Code** and **Verification URI** when triggered.
    - **Open Browser Button:** Launch the verification URL in the system browser.
    - **Loading Animation:** Shown while the backend polls for the authorization token.
    - **Safety:** Providers that fail the OAuth process are NOT saved to the configuration.
- **Editing:** Only Name can be modified. To refresh credentials, the user must re-authenticate.

## General Information

- **About FireBox:** Version info, links to documentation and repository.
- **Theme Settings:** Follows the system appearance; the frontend does not persist theme preferences.

## Data Source
Backend's `ListProviders`, `AddApiKeyProvider`, `AddOAuthProvider`, `SetModelEnabled`, and `DeleteProvider` APIs.
