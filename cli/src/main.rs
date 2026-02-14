//! Fire Box CLI — command-line interface for Fire Box gateway
//!
//! This tool provides a command-line interface to:
//! - Manage LLM providers (add, remove, list)
//! - Manage model rewrite rules (add, remove, list)
//! - Start a local OpenAI/Anthropic-compatible HTTP server
//! - View metrics and application authorizations

mod commands;
mod server;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "fire-box")]
#[command(about = "Fire Box - Stateful LLM gateway with auth and metrics", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage LLM providers
    Provider {
        #[command(subcommand)]
        action: ProviderCommands,
    },
    /// Manage model rewrite rules
    Model {
        #[command(subcommand)]
        action: ModelCommands,
    },
    /// Manage application authorizations
    App {
        #[command(subcommand)]
        action: AppCommands,
    },
    /// Start local HTTP server (OpenAI/Anthropic compatible)
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,
        
        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
    /// View current metrics
    Metrics,
}

#[derive(Subcommand)]
enum ProviderCommands {
    /// List all providers
    List,
    /// Add a new provider
    Add {
        /// Provider tag (unique identifier)
        tag: String,
        
        /// Provider type (openai, anthropic, dashscope, copilot)
        #[arg(short, long)]
        r#type: String,
        
        /// API key (optional, can be set via keyring)
        #[arg(short, long)]
        api_key: Option<String>,
        
        /// Base URL (optional, for custom endpoints)
        #[arg(short, long)]
        base_url: Option<String>,
    },
    /// Remove a provider
    Remove {
        /// Provider tag to remove
        tag: String,
    },
}

#[derive(Subcommand)]
enum ModelCommands {
    /// List all model mappings
    List,
    /// Add a model rewrite rule
    Add {
        /// Model tag (e.g., "gpt-4")
        model: String,
        
        /// Provider tag to route to
        #[arg(short, long)]
        provider: String,
        
        /// Provider's model name
        #[arg(short = 'm', long)]
        provider_model: String,
    },
    /// Remove a model mapping
    Remove {
        /// Model tag to remove
        model: String,
    },
}

#[derive(Subcommand)]
enum AppCommands {
    /// List registered applications (not applicable to local API)
    List,
    /// Revoke an application's access (not applicable to local API)
    Revoke {
        /// Application ID to revoke
        app_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Provider { action } => commands::handle_provider_command(action).await?,
        Commands::Model { action } => commands::handle_model_command(action).await?,
        Commands::App { action } => commands::handle_app_command(action).await?,
        Commands::Serve { port, host } => server::start_server(&host, port).await?,
        Commands::Metrics => commands::show_metrics().await?,
    }

    Ok(())
}
