# Native Graphical User Interface Application

This document delineates the architectural design and functional specifications of the FireBox native graphical user interface application. It is essential to note that the GUI application serves exclusively as a control surface, with all substantive functionality being implemented within the backend service and accessed through the Inter-Process Communication (IPC) protocols comprehensively defined in the CONTROL.md and CAPABILITY.md documentation.

## Dashboard Interface

The dashboard constitutes the primary interface through which users monitor system performance and resource utilization. This component is designed to present real-time metrics in an accessible and comprehensible format, thereby enabling users to maintain awareness of system activity. The metrics displayed encompass token consumption statistics, request volume data, and associated cost calculations. The real-time nature of this display ensures that users receive immediate feedback regarding the operational status and resource consumption of the FireBox service.

## Connections Management

The connections management interface provides users with comprehensive visibility into active client interactions with the FireBox service. This page presents a detailed enumeration of all current connections established by local programs, thereby facilitating monitoring of which applications are actively utilizing the service at any given moment. Such transparency enables users to identify unauthorized or unexpected access attempts and maintain awareness of system usage patterns.

## Model Configuration

The model configuration interface empowers users to establish and maintain model rewrite rules, which constitute a critical component of the routing infrastructure. Through this interface, users can define rules that govern how requests from local programs are directed to specific providers and models. These rules enable the system to perform model identifier rewriting, whereby virtual model identifiers specified by client applications are translated into actual provider-specific model identifiers. This abstraction layer provides flexibility in routing decisions and enables sophisticated load balancing and failover strategies.

## Provider Administration

The provider administration interface serves as the central management console for all artificial intelligence service providers integrated with the FireBox system. This comprehensive management facility accommodates three distinct categories of providers, each characterized by different authentication and operational paradigms. The first category comprises API key-based providers, exemplified by services such as OpenAI and Anthropic, which authenticate through static API keys. The second category encompasses OAuth-based providers, including GitHub Copilot and Qwen DashScope, which employ OAuth authentication flows to establish secure connections. The third category consists of local providers, such as Ollama, llama.cpp, and vLLM, which operate entirely on the local system without requiring external network connectivity. Through this interface, users can configure, enable, disable, and monitor all provider integrations.

## System Tray Menu

The system tray menu, which manifests as either a menu bar icon on macOS or a taskbar icon on Windows and Linux systems, provides convenient access to essential system functions. This menu displays the current operational status of the FireBox service, thereby enabling users to quickly ascertain whether the service is running, paused, or experiencing difficulties. Additionally, the menu provides a mechanism for users to restore or bring to focus the main application user interface window. Finally, the menu includes an exit action that enables users to terminate the FireBox application gracefully, ensuring proper cleanup of resources and closure of active connections.
