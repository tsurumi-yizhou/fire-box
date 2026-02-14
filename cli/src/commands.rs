//! Command handlers for Fire Box CLI

use crate::{AppCommands, ModelCommands, ProviderCommands};
use anyhow::{anyhow, Result};

pub async fn handle_provider_command(action: ProviderCommands) -> Result<()> {
    match action {
        ProviderCommands::List => {
            let config = common::config::Config::load_from_keyring();
            println!("Configured providers:");
            for p in &config.providers {
                println!(
                    "  {} ({:?}) - {}",
                    p.tag,
                    p.provider_type,
                    p.base_url.as_deref().unwrap_or("default endpoint")
                );
            }
        }
        ProviderCommands::Add {
            tag,
            r#type,
            api_key,
            base_url,
        } => {
            let provider_type = match r#type.to_lowercase().as_str() {
                "openai" => common::config::ProtocolType::OpenAI,
                "anthropic" => common::config::ProtocolType::Anthropic,
                "dashscope" => common::config::ProtocolType::DashScope,
                "copilot" => common::config::ProtocolType::Copilot,
                _ => return Err(anyhow!("Invalid provider type: {}", r#type)),
            };

            let mut config = common::config::Config::load_from_keyring();
            
            // Check for duplicate tag
            if config.providers.iter().any(|p| p.tag == tag) {
                return Err(anyhow!("Provider with tag '{}' already exists", tag));
            }

            // For DashScope, initiate OAuth if no API key provided
            if provider_type == common::config::ProtocolType::DashScope && api_key.is_none() {
                // Trigger preflight check which will start OAuth flow if needed
                let http = reqwest::Client::new();
                let dummy_creds_path = format!(".fire-box-creds/{}.json", tag);
                
                // Run preflight check to initiate OAuth
                if let Err(e) = common::protocols::dashscope::preflight_check(&http, &tag, &dummy_creds_path).await {
                    eprintln!("OAuth initialization failed: {}", e);
                }
            }

            config.providers.push(common::config::ProviderConfig {
                tag: tag.clone(),
                provider_type,
                base_url,
                oauth_creds_path: if provider_type == common::config::ProtocolType::DashScope {
                    Some(format!(".fire-box-creds/{}.json", tag))
                } else {
                    None
                },
            });
            
            // Store API key separately in keyring
            if let Some(key) = api_key
                && let Err(e) = common::keystore::store_provider_key(&tag, &key) {
                eprintln!("Warning: Failed to store API key in keyring: {}", e);
            }

            config.save_to_keyring()?;
            println!("Provider '{}' added successfully", tag);
        }
        ProviderCommands::Remove { tag } => {
            let mut config = common::config::Config::load_from_keyring();
            let before = config.providers.len();
            config.providers.retain(|p| p.tag != tag);
            let after = config.providers.len();

            if before == after {
                return Err(anyhow!("Provider '{}' not found", tag));
            }

            config.save_to_keyring()?;
            println!("Provider '{}' removed successfully", tag);
        }
    }
    Ok(())
}

pub async fn handle_model_command(action: ModelCommands) -> Result<()> {
    match action {
        ModelCommands::List => {
            let config = common::config::Config::load_from_keyring();
            println!("Model mappings:");
            for (model_tag, mappings) in &config.models {
                println!("  {}:", model_tag);
                for mapping in mappings {
                    println!("    → {} ({})", mapping.provider, mapping.model_id);
                }
            }
        }
        ModelCommands::Add {
            model,
            provider,
            provider_model,
        } => {
            let mut config = common::config::Config::load_from_keyring();

            // Verify provider exists
            if !config.providers.iter().any(|p| p.tag == provider) {
                return Err(anyhow!("Provider '{}' not found", provider));
            }

            let mapping = common::config::ProviderMapping {
                provider: provider.clone(),
                model_id: provider_model.clone(),
            };

            config
                .models
                .entry(model.clone())
                .or_default()
                .push(mapping);

            config.save_to_keyring()?;
            println!(
                "Model mapping added: {} → {} ({})",
                model, provider, provider_model
            );
        }
        ModelCommands::Remove { model } => {
            let mut config = common::config::Config::load_from_keyring();

            if config.models.remove(&model).is_none() {
                return Err(anyhow!("Model mapping '{}' not found", model));
            }

            config.save_to_keyring()?;
            println!("Model mapping '{}' removed successfully", model);
        }
    }
    Ok(())
}

pub async fn handle_app_command(action: AppCommands) -> Result<()> {
    match action {
        AppCommands::List => {
            println!("App authorization is handled at the API level with token-based access.");
            println!("All local requests to localhost:8080 are accepted with any API key.");
        }
        AppCommands::Revoke { app_id: _ } => {
            println!("App authorization is handle at the API level.");
            println!("To revoke access, use firewall rules or restart the server.");
        }
    }
    Ok(())
}

pub async fn show_metrics() -> Result<()> {
    let state = common::CoreState::new().await?;
    let snapshot = state.metrics.snapshot().await;

    println!("Fire Box Metrics:");
    println!("  Total requests:      {}", snapshot.total_requests);
    println!("  Input tokens:        {}", snapshot.total_input_tokens);
    println!("  Output tokens:       {}", snapshot.total_output_tokens);
    println!("  Active connections:  {}", snapshot.active_connections);

    Ok(())
}
