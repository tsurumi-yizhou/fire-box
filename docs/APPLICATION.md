# Native GUI Application

The GUI app is only a control surface; all functionality is implemented in the backend service and accessed via the IPC protocol defined in PROTOCOL.md.

## Dashboard
The dashboard should display real-time metrics, including tokens, requests, and cost.

## Connections
This page lists all active connections from local programs.

## Models
Users can define model rewrite rules here. Based on these rules, requests from local programs are routed to the actual provider and model, and the virtual model ID is rewritten.

## Providers
Users can manage all providers here, including:
- API key-based providers (e.g., OpenAI, Anthropic)
- OAuth-based providers (e.g., GitHub Copilot, Qwen DashScope)
- Local providers (e.g., Ollama, llama.cpp, vLLM)

## Tray Menu
The tray icon menu (menu bar or taskbar) should show service status, provide a way to return to the app UI, and include an Exit action.
