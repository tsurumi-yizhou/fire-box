use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub log: LogConfig,
    pub providers: Vec<ProviderConfig>,
    pub channels: Vec<ChannelConfig>,
    pub routes: Vec<RouteConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogConfig {
    pub level: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub tag: String,
    #[serde(rename = "type")]
    pub provider_type: ProtocolType,
    pub api_key: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelConfig {
    #[serde(rename = "type")]
    pub channel_type: ProtocolType,
    pub tag: String,
    pub port: u16,
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouteConfig {
    /// Which channel tags this route applies to. If absent, matches all channels.
    #[serde(default)]
    pub channel: Vec<String>,
    /// Keyword matching against user message content.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Ordered list of provider+model to try.
    pub select: Vec<SelectConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SelectConfig {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProtocolType {
    OpenAI,
    Anthropic,
    DashScope,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&content)?;
        Ok(config)
    }
}
