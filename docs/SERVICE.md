# FireBox Service Design

This document presents a comprehensive examination of the architectural design and implementation of the FireBox Service, designated as `firebox-service`. The service constitutes a cross-platform background daemon implemented in the Rust programming language, serving as a unified gateway for artificial intelligence capabilities. Its responsibilities encompass provider management, request routing, and access control, thereby providing a cohesive interface for diverse client applications.

## Architectural Framework

The service adheres to a layered architectural paradigm, wherein distinct functional concerns are separated into discrete layers that interact through well-defined interfaces. This architectural approach facilitates maintainability, testability, and extensibility whilst ensuring clear separation of responsibilities.

```mermaid
graph TD
    Client[Client Apps] <--> IPC[IPC Layer]
    Control[Control App] <--> IPC
    
    subgraph Service
        IPC <--> Middleware
        Middleware <--> Providers
        
        subgraph Middleware
            Auth[Access Control]
            Route[Routing & Failover]
            Metrics[Metrics & Logging]
            Config[Encrypted Config]
        end
        
        subgraph Providers
            OpenAI[OpenAI / Compatible]
            Anthropic[Anthropic]
            Copilot[GitHub Copilot]
            DashScope[DashScope (Qwen)]
            Llama[llama.cpp]
        end
    end
    
    Providers <--> Cloud[Cloud APIs]
    Llama <--> GPU[Local GPU]
```

## Middleware Layer Architecture

The middleware layer constitutes a critical component of the service architecture, responsible for handling cross-cutting concerns that apply uniformly across all requests before they reach specific model providers. This layer implements functionality that transcends individual provider implementations, thereby ensuring consistent behavior and policy enforcement throughout the system.

### 1. Configuration and Storage Management

The configuration and storage subsystem implements secure persistence mechanisms for both application configuration and sensitive credentials. Application configuration, which encompasses the provider index and routing rules, is stored in an encrypted file designated as `fire-box-store.enc`, located within the user's configuration directory. This file employs AES-256-GCM encryption to ensure confidentiality and integrity of configuration data. Sensitive credentials, including API keys and OAuth tokens, are never persisted in plain text files. Instead, the system leverages the operating system's native credential storage mechanisms: macOS Keychain on Apple platforms, Windows Credential Manager on Microsoft platforms, and the Secret Service API on Linux systems. This approach ensures that credentials benefit from the security protections provided by the underlying operating system.

### 2. Routing Infrastructure

The routing subsystem implements an abstraction layer that decouples client applications from specific provider implementations. Rather than requesting models by their physical identifiers, clients reference models through aliases such as `coding-model` or `fast-chat`. This indirection enables flexible configuration changes without requiring modifications to client applications. Furthermore, the routing system supports failover capabilities through ordered lists of targets. When a primary provider becomes unavailable due to network errors, rate limiting, or other transient failures, the service automatically retries the request with subsequent targets in the configured chain, thereby enhancing reliability and availability.

### 3. Metrics Collection and Aggregation

The metrics subsystem implements comprehensive instrumentation throughout the request processing pipeline, collecting granular data for every request processed by the service. The collected metrics encompass token usage statistics, distinguishing between prompt tokens and completion tokens; latency measurements capturing request processing duration; estimated cost calculations based on provider pricing models; and error rate tracking to identify reliability issues. These metrics are aggregated in memory and made available through the Control Protocol, enabling real-time monitoring and historical analysis of system performance and resource consumption.

### 4. Metadata Management

The metadata subsystem serves as the authoritative source for the capabilities of physical models, playing a crucial role in validating routing contracts. It automatically retrieves and caches model metadata from the `models.dev` service, maintaining current information regarding context window sizes, modality support (e.g., vision), pricing structures, and other technical specifications for public cloud models. For local models, this subsystem extracts metadata directly from model files (e.g., GGUF headers). This comprehensive knowledge base enables the routing subsystem to strictly enforce capability contracts, ensuring that physical models assigned to a virtual route genuinely support the required features.

## Provider Layer Architecture

The provider layer implements adapters that translate between the service's unified internal interface and the diverse protocols employed by various backend artificial intelligence services. This abstraction enables the service to support multiple providers whilst presenting a consistent interface to higher layers of the system.

### Supported Provider Implementations

The service currently supports five distinct categories of providers, each characterized by different authentication mechanisms, communication protocols, and operational characteristics. The OpenAI provider supports API key-based authentication and implements both chat completion and embedding capabilities with streaming support. Notably, this provider is designed to accommodate not only the official OpenAI API but also any service that implements an OpenAI-compatible interface, including Ollama and vLLM deployments. The Anthropic provider implements native integration with Claude API services, supporting API key authentication and providing chat completion capabilities with streaming support.

The GitHub Copilot provider implements OAuth-based authentication through a device flow mechanism, enabling users to authorize the service to access Copilot capabilities on their behalf. Once authenticated, the provider proxies requests to the Copilot API, translating between the service's internal protocol and Copilot's specific requirements. Similarly, the DashScope provider implements OAuth authentication using Alibaba's device flow mechanism and communicates using Qwen's native protocol, providing access to DashScope's model offerings.

The llama.cpp provider represents a fundamentally different operational model, as it manages a local `llama-server` child process rather than communicating with remote services. This provider enables entirely offline operation, executing models directly on the user's hardware without requiring network connectivity. It supports both chat completion and embedding capabilities, providing functionality comparable to cloud-based providers whilst maintaining complete data privacy.

### Provider Abstraction Interface

All provider implementations conform to a unified `Provider` trait that defines the standard operations available across all providers. The `complete` operation implements non-streaming chat completion, accepting a request and returning a complete response. The `complete_stream` operation implements streaming chat completion using Server-Sent Events (SSE), enabling incremental delivery of response content. The `embed` operation generates text embeddings, converting textual input into vector representations suitable for semantic search and similarity calculations. Finally, the `list_models` operation implements model discovery, enabling the service to enumerate the models available through each provider.

## Security Architecture

The security architecture implements multiple layers of protection to ensure that only authorized applications can access the service and that sensitive data remains protected throughout its lifecycle.

### Access Control Implementation (Trust On First Use)

The access control mechanism implements a "Trust On First Use" paradigm that balances security requirements with user convenience. The process commences with identification, wherein the service identifies calling processes through operating system-level peer credentials obtained from the IPC connection. These credentials enable the service to determine the executable path and, on platforms that support it, verify code signatures to ensure application authenticity.

Following identification, the service performs verification by consulting its persistent allowlist database to determine whether the requesting application has been previously authorized. This verification step enables immediate access for known applications whilst triggering additional authorization procedures for unknown clients. The authorization phase implements the interactive approval workflow: if an application is found in the allowlist, the request proceeds immediately; if the application has been explicitly denied, the request is rejected; if the application is unknown, the service suspends the request and launches a native Helper GUI to solicit user approval.

The enforcement phase implements the user's decision: if the user approves the request, the application is added to the persistent allowlist and the connection proceeds; if the user denies the request, the connection is rejected and the application remains unauthorized. This approach ensures that users maintain explicit control over which applications may access artificial intelligence capabilities whilst avoiding repetitive authorization prompts for trusted applications.

### Data Privacy Protections

The service implements multiple mechanisms to protect user data and maintain privacy. Local processing capabilities, exemplified by the llama.cpp provider, enable entirely offline operation wherein models execute directly on the user's hardware without transmitting data to external services. For cloud-based providers, the service establishes direct connections to provider APIs, ensuring that no traffic traverses third-party relay servers or intermediaries. This architecture minimizes the attack surface and reduces the number of entities with potential access to user data, thereby enhancing overall privacy and security.
