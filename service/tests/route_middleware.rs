//! Tests for Route Module (Model Routing and Failover)

use firebox_service::middleware::route::{
    ModelEnabledState, RouteRule, RouteTarget, delete_route_rules, get_all_rules, get_next_target,
    get_route_rules, is_model_enabled, list_enabled_models, load_enabled_models, resolve_alias,
    save_enabled_models, set_route_rules, toggle_model,
};

// RouteTarget tests
#[test]
fn route_target_basic() {
    let target = RouteTarget {
        provider_id: "openai".to_string(),
        model_id: "gpt-4".to_string(),
    };

    assert_eq!(target.provider_id, "openai");
    assert_eq!(target.model_id, "gpt-4");
}

#[test]
fn route_target_clone() {
    let target = RouteTarget {
        provider_id: "anthropic".to_string(),
        model_id: "claude-3".to_string(),
    };

    let cloned = target.clone();
    assert_eq!(target.provider_id, cloned.provider_id);
    assert_eq!(target.model_id, cloned.model_id);
}

#[test]
fn route_target_debug() {
    let target = RouteTarget {
        provider_id: "openai".to_string(),
        model_id: "gpt-4".to_string(),
    };

    let debug_str = format!("{:?}", target);
    assert!(debug_str.contains("openai"));
    assert!(debug_str.contains("gpt-4"));
}

#[test]
fn route_target_different_providers() {
    let providers = vec!["openai", "anthropic", "copilot", "dashscope", "llamacpp"];

    for provider in providers {
        let target = RouteTarget {
            provider_id: provider.to_string(),
            model_id: "model".to_string(),
        };
        assert_eq!(target.provider_id, provider);
    }
}

#[test]
fn route_target_same_provider_different_models() {
    let models = vec!["gpt-4", "gpt-3.5", "gpt-4-turbo"];

    for model in models {
        let target = RouteTarget {
            provider_id: "openai".to_string(),
            model_id: model.to_string(),
        };
        assert_eq!(target.model_id, model);
    }
}

// RouteRule tests
#[test]
fn route_rule_single_target() {
    let rule = RouteRule {
        alias: "smart-model".to_string(),
        targets: vec![RouteTarget {
            provider_id: "openai".to_string(),
            model_id: "gpt-4".to_string(),
        }],
    };

    assert_eq!(rule.alias, "smart-model");
    assert_eq!(rule.targets.len(), 1);
}

#[test]
fn route_rule_multiple_targets() {
    let rule = RouteRule {
        alias: "failover-model".to_string(),
        targets: vec![
            RouteTarget {
                provider_id: "openai".to_string(),
                model_id: "gpt-4".to_string(),
            },
            RouteTarget {
                provider_id: "anthropic".to_string(),
                model_id: "claude-3".to_string(),
            },
            RouteTarget {
                provider_id: "dashscope".to_string(),
                model_id: "qwen-max".to_string(),
            },
        ],
    };

    assert_eq!(rule.targets.len(), 3);
    assert_eq!(rule.targets[0].provider_id, "openai");
    assert_eq!(rule.targets[1].provider_id, "anthropic");
    assert_eq!(rule.targets[2].provider_id, "dashscope");
}

#[test]
fn route_rule_clone() {
    let rule = RouteRule {
        alias: "clone-rule".to_string(),
        targets: vec![RouteTarget {
            provider_id: "openai".to_string(),
            model_id: "gpt-4".to_string(),
        }],
    };

    let cloned = rule.clone();
    assert_eq!(rule.alias, cloned.alias);
    assert_eq!(rule.targets.len(), cloned.targets.len());
}

#[test]
fn route_rule_debug() {
    let rule = RouteRule {
        alias: "debug-rule".to_string(),
        targets: vec![],
    };

    let debug_str = format!("{:?}", rule);
    assert!(debug_str.contains("debug-rule"));
}

#[test]
fn route_rule_empty_targets() {
    let rule = RouteRule {
        alias: "empty-rule".to_string(),
        targets: vec![],
    };

    assert!(rule.targets.is_empty());
}

// ModelEnabledState tests
#[test]
fn model_enabled_state_default() {
    let state = ModelEnabledState::default();
    assert!(state.enabled_models.is_empty());
}

#[test]
fn model_enabled_state_with_models() {
    let mut enabled_models = std::collections::HashMap::new();
    enabled_models.insert(
        "openai".to_string(),
        vec!["gpt-4".to_string(), "gpt-3.5".to_string()],
    );

    let state = ModelEnabledState { enabled_models };

    assert!(state.enabled_models.contains_key("openai"));
    assert_eq!(state.enabled_models.get("openai").unwrap().len(), 2);
}

#[test]
fn model_enabled_state_clone() {
    let mut enabled_models = std::collections::HashMap::new();
    enabled_models.insert("openai".to_string(), vec!["gpt-4".to_string()]);

    let state = ModelEnabledState { enabled_models };
    let cloned = state.clone();

    assert_eq!(state.enabled_models.len(), cloned.enabled_models.len());
}

// Route rules management tests
#[tokio::test]
async fn set_and_get_route_rules() {
    let targets = vec![
        RouteTarget {
            provider_id: "openai".to_string(),
            model_id: "gpt-4".to_string(),
        },
        RouteTarget {
            provider_id: "anthropic".to_string(),
            model_id: "claude-3".to_string(),
        },
    ];

    let result = set_route_rules("test-alias", targets).await;
    assert!(result.is_ok());

    let retrieved = get_route_rules("test-alias").await.unwrap();
    assert!(retrieved.is_some());
    let rule = retrieved.unwrap();
    assert_eq!(rule.alias, "test-alias");
    assert_eq!(rule.targets.len(), 2);
}

#[tokio::test]
async fn get_nonexistent_route_rules() {
    let result = get_route_rules("nonexistent-alias").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_delete_route_rules() {
    // First set
    let targets = vec![RouteTarget {
        provider_id: "openai".to_string(),
        model_id: "gpt-4".to_string(),
    }];
    set_route_rules("delete-test", targets).await.unwrap();

    // Then delete
    let result = delete_route_rules("delete-test").await;
    assert!(result.is_ok());

    // Verify deleted
    let retrieved = get_route_rules("delete-test").await.unwrap();
    assert!(retrieved.is_none());
}

#[tokio::test]
async fn test_get_all_rules() {
    // Add some rules
    set_route_rules(
        "rule1",
        vec![RouteTarget {
            provider_id: "openai".to_string(),
            model_id: "gpt-4".to_string(),
        }],
    )
    .await
    .unwrap();

    set_route_rules(
        "rule2",
        vec![RouteTarget {
            provider_id: "anthropic".to_string(),
            model_id: "claude-3".to_string(),
        }],
    )
    .await
    .unwrap();

    let all = get_all_rules().await.unwrap();
    assert!(all.len() >= 2);
}

#[tokio::test]
async fn update_existing_route_rules() {
    // Set initial rules
    let initial_targets = vec![RouteTarget {
        provider_id: "openai".to_string(),
        model_id: "gpt-3.5".to_string(),
    }];
    set_route_rules("update-test", initial_targets)
        .await
        .unwrap();

    // Update rules
    let updated_targets = vec![
        RouteTarget {
            provider_id: "openai".to_string(),
            model_id: "gpt-4".to_string(),
        },
        RouteTarget {
            provider_id: "anthropic".to_string(),
            model_id: "claude-3".to_string(),
        },
    ];
    set_route_rules("update-test", updated_targets)
        .await
        .unwrap();

    // Verify update
    let retrieved = get_route_rules("update-test").await.unwrap().unwrap();
    assert_eq!(retrieved.targets.len(), 2);
    assert_eq!(retrieved.targets[0].model_id, "gpt-4");
}

// Alias resolution tests
#[tokio::test]
async fn resolve_alias_without_rule() {
    // Without a rule, alias should resolve to default provider with alias as model
    let result = resolve_alias("gpt-4").await.unwrap();
    assert_eq!(result, ("default".to_string(), "gpt-4".to_string()));
}

#[tokio::test]
async fn resolve_alias_with_rule() {
    let targets = vec![RouteTarget {
        provider_id: "openai".to_string(),
        model_id: "gpt-4-turbo".to_string(),
    }];
    set_route_rules("my-gpt4", targets).await.unwrap();

    let result = resolve_alias("my-gpt4").await.unwrap();
    assert_eq!(result.0, "openai");
    assert_eq!(result.1, "gpt-4-turbo");
}

#[tokio::test]
async fn resolve_alias_returns_first_target() {
    let targets = vec![
        RouteTarget {
            provider_id: "openai".to_string(),
            model_id: "gpt-4".to_string(),
        },
        RouteTarget {
            provider_id: "anthropic".to_string(),
            model_id: "claude-3".to_string(),
        },
    ];
    set_route_rules("multi-target", targets).await.unwrap();

    let result = resolve_alias("multi-target").await.unwrap();
    // Should return first target
    assert_eq!(result.0, "openai");
    assert_eq!(result.1, "gpt-4");
}

// Failover tests
#[tokio::test]
async fn get_next_target_basic() {
    let targets = vec![
        RouteTarget {
            provider_id: "openai".to_string(),
            model_id: "gpt-4".to_string(),
        },
        RouteTarget {
            provider_id: "anthropic".to_string(),
            model_id: "claude-3".to_string(),
        },
    ];
    set_route_rules("failover-test", targets).await.unwrap();

    let next = get_next_target("failover-test", "openai").await.unwrap();
    assert!(next.is_some());
    let (provider, model) = next.unwrap();
    assert_eq!(provider, "anthropic");
    assert_eq!(model, "claude-3");
}

#[tokio::test]
async fn get_next_target_last_provider() {
    let targets = vec![
        RouteTarget {
            provider_id: "openai".to_string(),
            model_id: "gpt-4".to_string(),
        },
        RouteTarget {
            provider_id: "anthropic".to_string(),
            model_id: "claude-3".to_string(),
        },
    ];
    set_route_rules("failover-last", targets).await.unwrap();

    let next = get_next_target("failover-last", "anthropic").await.unwrap();
    assert!(next.is_none());
}

#[tokio::test]
async fn get_next_target_nonexistent_alias() {
    let next = get_next_target("nonexistent", "openai").await.unwrap();
    assert!(next.is_none());
}

#[tokio::test]
async fn get_next_target_single_rule() {
    let targets = vec![RouteTarget {
        provider_id: "openai".to_string(),
        model_id: "gpt-4".to_string(),
    }];
    set_route_rules("single-target", targets).await.unwrap();

    let next = get_next_target("single-target", "openai").await.unwrap();
    assert!(next.is_none());
}

#[tokio::test]
async fn get_next_target_three_providers() {
    let targets = vec![
        RouteTarget {
            provider_id: "openai".to_string(),
            model_id: "gpt-4".to_string(),
        },
        RouteTarget {
            provider_id: "anthropic".to_string(),
            model_id: "claude-3".to_string(),
        },
        RouteTarget {
            provider_id: "dashscope".to_string(),
            model_id: "qwen-max".to_string(),
        },
    ];
    set_route_rules("three-targets", targets).await.unwrap();

    // From first to second
    let next1 = get_next_target("three-targets", "openai").await.unwrap();
    assert_eq!(next1.unwrap().0, "anthropic");

    // From second to third
    let next2 = get_next_target("three-targets", "anthropic").await.unwrap();
    assert_eq!(next2.unwrap().0, "dashscope");

    // From third, no more
    let next3 = get_next_target("three-targets", "dashscope").await.unwrap();
    assert!(next3.is_none());
}

// Model enabled state tests
#[tokio::test]
async fn save_and_load_enabled_models() {
    let models = vec!["gpt-4".to_string(), "gpt-3.5".to_string()];
    let result = save_enabled_models("openai", &models).await;
    assert!(result.is_ok());

    let loaded = load_enabled_models("openai").await;
    assert!(loaded.is_some());
    let loaded_models = loaded.unwrap();
    assert_eq!(loaded_models.len(), 2);
}

#[tokio::test]
async fn load_nonexistent_enabled_models() {
    let loaded = load_enabled_models("nonexistent-provider").await;
    assert!(loaded.is_none());
}

#[tokio::test]
async fn is_model_enabled_default() {
    // When no state is saved, all models should be enabled
    assert!(is_model_enabled("openai", "gpt-4").await);
    assert!(is_model_enabled("openai", "any-model").await);
}

#[tokio::test]
async fn is_model_enabled_with_specific_models() {
    save_enabled_models("openai", &["gpt-4".to_string()])
        .await
        .unwrap();

    assert!(is_model_enabled("openai", "gpt-4").await);
    assert!(!is_model_enabled("openai", "gpt-3.5").await);
}

#[tokio::test]
async fn toggle_model_enable() {
    let all_models = vec!["gpt-4".to_string(), "gpt-3.5".to_string()];

    // Start with none enabled
    save_enabled_models("openai", &[]).await.unwrap();

    // Enable gpt-4
    toggle_model("openai", "gpt-4", true, &all_models)
        .await
        .unwrap();

    assert!(is_model_enabled("openai", "gpt-4").await);
    assert!(!is_model_enabled("openai", "gpt-3.5").await);
}

#[tokio::test]
async fn toggle_model_disable() {
    let all_models = vec!["gpt-4".to_string(), "gpt-3.5".to_string()];

    // Start with both enabled
    save_enabled_models("openai", &all_models).await.unwrap();

    // Disable gpt-3.5
    toggle_model("openai", "gpt-3.5", false, &all_models)
        .await
        .unwrap();

    assert!(is_model_enabled("openai", "gpt-4").await);
    assert!(!is_model_enabled("openai", "gpt-3.5").await);
}

#[tokio::test]
async fn list_enabled_models_all() {
    let all_models = vec!["gpt-4".to_string(), "gpt-3.5".to_string()];

    // None saved means all enabled
    let enabled = list_enabled_models("openai", &all_models).await;
    assert_eq!(enabled.len(), 2);
}

#[tokio::test]
async fn list_enabled_models_specific() {
    let all_models = vec![
        "gpt-4".to_string(),
        "gpt-3.5".to_string(),
        "gpt-4-turbo".to_string(),
    ];
    let enabled_models = vec!["gpt-4".to_string(), "gpt-4-turbo".to_string()];

    save_enabled_models("openai", &enabled_models)
        .await
        .unwrap();

    let enabled = list_enabled_models("openai", &all_models).await;
    assert_eq!(enabled.len(), 2);
    assert!(enabled.contains(&"gpt-4".to_string()));
    assert!(enabled.contains(&"gpt-4-turbo".to_string()));
}

#[tokio::test]
async fn toggle_model_add_new() {
    let all_models = vec!["gpt-4".to_string(), "new-model".to_string()];

    // Start with only gpt-4
    save_enabled_models("openai", &["gpt-4".to_string()])
        .await
        .unwrap();

    // Enable new model
    toggle_model("openai", "new-model", true, &all_models)
        .await
        .unwrap();

    assert!(is_model_enabled("openai", "gpt-4").await);
    assert!(is_model_enabled("openai", "new-model").await);
}

#[tokio::test]
async fn toggle_model_disable_last_one() {
    let all_models = vec!["only-model".to_string()];

    // Start with only model enabled
    save_enabled_models("openai", &["only-model".to_string()])
        .await
        .unwrap();

    // Disable it
    toggle_model("openai", "only-model", false, &all_models)
        .await
        .unwrap();

    assert!(!is_model_enabled("openai", "only-model").await);
}

// Edge cases
#[test]
fn route_target_empty_strings() {
    let target = RouteTarget {
        provider_id: "".to_string(),
        model_id: "".to_string(),
    };
    assert_eq!(target.provider_id, "");
    assert_eq!(target.model_id, "");
}

#[test]
fn route_rule_unicode_alias() {
    let rule = RouteRule {
        alias: "智能模型".to_string(),
        targets: vec![],
    };
    assert_eq!(rule.alias, "智能模型");
}

#[test]
fn route_target_special_chars() {
    let target = RouteTarget {
        provider_id: "open-ai".to_string(),
        model_id: "gpt-4-turbo-preview".to_string(),
    };
    assert!(target.provider_id.contains('-'));
    assert!(target.model_id.contains('-'));
}

#[test]
fn many_targets_in_rule() {
    let mut targets = Vec::new();
    for i in 0..50 {
        targets.push(RouteTarget {
            provider_id: format!("provider-{}", i),
            model_id: format!("model-{}", i),
        });
    }

    let rule = RouteRule {
        alias: "many-targets".to_string(),
        targets,
    };

    assert_eq!(rule.targets.len(), 50);
}

#[tokio::test]
async fn enabled_models_empty_list() {
    save_enabled_models("openai", &[]).await.unwrap();

    // Empty list means none enabled
    assert!(!is_model_enabled("openai", "any-model").await);
}

#[tokio::test]
async fn resolve_alias_with_unicode() {
    let targets = vec![RouteTarget {
        provider_id: "dashscope".to_string(),
        model_id: "qwen-max".to_string(),
    }];
    set_route_rules("中文别名", targets).await.unwrap();

    let result = resolve_alias("中文别名").await.unwrap();
    assert_eq!(result.0, "dashscope");
}
