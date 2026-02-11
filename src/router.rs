/// Routing engine.
/// Matches incoming requests to provider+model candidates based on config rules.
use crate::config::{Config, RouteConfig, SelectConfig};
use crate::protocol::UnifiedRequest;

/// Find the ordered list of (provider_tag, model) to try for a given request.
pub fn resolve_route(
    config: &Config,
    channel_tag: &str,
    request: &UnifiedRequest,
) -> Vec<SelectConfig> {
    let user_text = request.user_text().to_lowercase();

    for route in &config.routes {
        if matches_route(route, channel_tag, &user_text) {
            return route.select.clone();
        }
    }

    // Should not reach here if config has a catch-all, but fall back to empty.
    Vec::new()
}

fn matches_route(route: &RouteConfig, channel_tag: &str, user_text: &str) -> bool {
    // Check channel filter.
    if !route.channel.is_empty() && !route.channel.iter().any(|c| c == channel_tag) {
        return false;
    }

    // Check keyword filter.
    if !route.keywords.is_empty() {
        let has_keyword = route
            .keywords
            .iter()
            .any(|kw| user_text.contains(&kw.to_lowercase()));
        if !has_keyword {
            return false;
        }
    }

    // All conditions matched (or none specified = catch-all).
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;
    use crate::protocol::*;

    fn test_config() -> Config {
        Config {
            log: LogConfig {
                level: "info".into(),
            },
            providers: vec![],
            channels: vec![],
            routes: vec![
                RouteConfig {
                    channel: vec!["coding-openai".into()],
                    keywords: vec!["def".into(), "class".into()],
                    select: vec![SelectConfig {
                        provider: "Anthropic".into(),
                        model: "claude-3".into(),
                    }],
                },
                RouteConfig {
                    channel: vec![],
                    keywords: vec![],
                    select: vec![SelectConfig {
                        provider: "OpenAI".into(),
                        model: "gpt-4".into(),
                    }],
                },
            ],
        }
    }

    #[test]
    fn test_keyword_match() {
        let cfg = test_config();
        let req = UnifiedRequest {
            model: "test".into(),
            messages: vec![UnifiedMessage {
                role: "user".into(),
                content: MessageContent::Text("Please define a class Foo".into()),
            }],
            stream: false,
            max_tokens: None,
            temperature: None,
            files: vec![],
        };
        let result = resolve_route(&cfg, "coding-openai", &req);
        assert_eq!(result[0].model, "claude-3");
    }

    #[test]
    fn test_catchall() {
        let cfg = test_config();
        let req = UnifiedRequest {
            model: "test".into(),
            messages: vec![UnifiedMessage {
                role: "user".into(),
                content: MessageContent::Text("Hello world".into()),
            }],
            stream: false,
            max_tokens: None,
            temperature: None,
            files: vec![],
        };
        let result = resolve_route(&cfg, "random-channel", &req);
        assert_eq!(result[0].model, "gpt-4");
    }
}
