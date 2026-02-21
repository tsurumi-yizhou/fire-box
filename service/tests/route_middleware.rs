//! Tests for Route Module (Model Routing and Failover)

use firebox_service::middleware::route::{
    ModelEnabledState, RouteRule, RouteTarget, delete_route_rules, get_all_rules, get_next_target,
    get_route_rules, init, is_model_enabled, list_enabled_models, load_enabled_models,
    resolve_alias, save_enabled_models, set_route_rules, toggle_model,
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

// Route initialization tests
#[test]
fn init_route_storage() {
    let result = init();
    assert!(result.is_ok());
}

#[test]
fn init_multiple_times() {
    // Should be idempotent
    let result1 = init();
    let result2 = init();
    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

// Route rules management tests
#[test]
fn set_and_get_route_rules() {
    let _ = init();

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

    let result = set_route_rules("test-alias", targets);
    assert!(result.is_ok());

    let retrieved = get_route_rules("test-alias").unwrap();
    assert!(retrieved.is_some());
    let rule = retrieved.unwrap();
    assert_eq!(rule.alias, "test-alias");
    assert_eq!(rule.targets.len(), 2);
}

#[test]
fn get_nonexistent_route_rules() {
    let _ = init();

    let result = get_route_rules("nonexistent-alias").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_delete_route_rules() {
    let _ = init();

    // First set
    let targets = vec![RouteTarget {
        provider_id: "openai".to_string(),
        model_id: "gpt-4".to_string(),
    }];
    set_route_rules("delete-test", targets).unwrap();

    // Then delete
    let result = delete_route_rules("delete-test");
    assert!(result.is_ok());

    // Verify deleted
    let retrieved = get_route_rules("delete-test").unwrap();
    assert!(retrieved.is_none());
}

#[test]
fn test_get_all_rules() {
    let _ = init();

    // Add some rules
    set_route_rules(
        "rule1",
        vec![RouteTarget {
            provider_id: "openai".to_string(),
            model_id: "gpt-4".to_string(),
        }],
    )
    .unwrap();

    set_route_rules(
        "rule2",
        vec![RouteTarget {
            provider_id: "anthropic".to_string(),
            model_id: "claude-3".to_string(),
        }],
    )
    .unwrap();

    let all = get_all_rules().unwrap();
    assert!(all.len() >= 2);
}

#[test]
fn update_existing_route_rules() {
    let _ = init();

    // Set initial rules
    let initial_targets = vec![RouteTarget {
        provider_id: "openai".to_string(),
        model_id: "gpt-3.5".to_string(),
    }];
    set_route_rules("update-test", initial_targets).unwrap();

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
    set_route_rules("update-test", updated_targets).unwrap();

    // Verify update
    let retrieved = get_route_rules("update-test").unwrap().unwrap();
    assert_eq!(retrieved.targets.len(), 2);
    assert_eq!(retrieved.targets[0].model_id, "gpt-4");
}

// Alias resolution tests
#[test]
fn resolve_alias_without_rule() {
    let _ = init();

    // Without a rule, alias should resolve to default provider with alias as model
    let result = resolve_alias("gpt-4").unwrap();
    assert_eq!(result, ("default".to_string(), "gpt-4".to_string()));
}

#[test]
fn resolve_alias_with_rule() {
    let _ = init();

    let targets = vec![RouteTarget {
        provider_id: "openai".to_string(),
        model_id: "gpt-4-turbo".to_string(),
    }];
    set_route_rules("my-gpt4", targets).unwrap();

    let result = resolve_alias("my-gpt4").unwrap();
    assert_eq!(result.0, "openai");
    assert_eq!(result.1, "gpt-4-turbo");
}

#[test]
fn resolve_alias_returns_first_target() {
    let _ = init();

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
    set_route_rules("multi-target", targets).unwrap();

    let result = resolve_alias("multi-target").unwrap();
    // Should return first target
    assert_eq!(result.0, "openai");
    assert_eq!(result.1, "gpt-4");
}

// Failover tests
#[test]
fn get_next_target_basic() {
    let _ = init();

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
    set_route_rules("failover-test", targets).unwrap();

    let next = get_next_target("failover-test", "openai").unwrap();
    assert!(next.is_some());
    let (provider, model) = next.unwrap();
    assert_eq!(provider, "anthropic");
    assert_eq!(model, "claude-3");
}

#[test]
fn get_next_target_last_provider() {
    let _ = init();

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
    set_route_rules("failover-last", targets).unwrap();

    let next = get_next_target("failover-last", "anthropic").unwrap();
    assert!(next.is_none());
}

#[test]
fn get_next_target_nonexistent_alias() {
    let _ = init();

    let next = get_next_target("nonexistent", "openai").unwrap();
    assert!(next.is_none());
}

#[test]
fn get_next_target_single_rule() {
    let _ = init();

    let targets = vec![RouteTarget {
        provider_id: "openai".to_string(),
        model_id: "gpt-4".to_string(),
    }];
    set_route_rules("single-target", targets).unwrap();

    let next = get_next_target("single-target", "openai").unwrap();
    assert!(next.is_none());
}

#[test]
fn get_next_target_three_providers() {
    let _ = init();

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
    set_route_rules("three-targets", targets).unwrap();

    // From first to second
    let next1 = get_next_target("three-targets", "openai").unwrap();
    assert_eq!(next1.unwrap().0, "anthropic");

    // From second to third
    let next2 = get_next_target("three-targets", "anthropic").unwrap();
    assert_eq!(next2.unwrap().0, "dashscope");

    // From third, no more
    let next3 = get_next_target("three-targets", "dashscope").unwrap();
    assert!(next3.is_none());
}

// Model enabled state tests
#[test]
fn save_and_load_enabled_models() {
    let _ = init();

    let models = vec!["gpt-4".to_string(), "gpt-3.5".to_string()];
    let result = save_enabled_models("openai", &models);
    assert!(result.is_ok());

    let loaded = load_enabled_models("openai");
    assert!(loaded.is_some());
    let loaded_models = loaded.unwrap();
    assert_eq!(loaded_models.len(), 2);
}

#[test]
fn load_nonexistent_enabled_models() {
    let _ = init();

    let loaded = load_enabled_models("nonexistent-provider");
    assert!(loaded.is_none());
}

#[test]
fn is_model_enabled_default() {
    let _ = init();

    // When no state is saved, all models should be enabled
    assert!(is_model_enabled("openai", "gpt-4"));
    assert!(is_model_enabled("openai", "any-model"));
}

#[test]
fn is_model_enabled_with_specific_models() {
    let _ = init();

    save_enabled_models("openai", &["gpt-4".to_string()]).unwrap();

    assert!(is_model_enabled("openai", "gpt-4"));
    assert!(!is_model_enabled("openai", "gpt-3.5"));
}

#[test]
fn toggle_model_enable() {
    let _ = init();

    let all_models = vec!["gpt-4".to_string(), "gpt-3.5".to_string()];

    // Start with none enabled
    save_enabled_models("openai", &[]).unwrap();

    // Enable gpt-4
    toggle_model("openai", "gpt-4", true, &all_models).unwrap();

    assert!(is_model_enabled("openai", "gpt-4"));
    assert!(!is_model_enabled("openai", "gpt-3.5"));
}

#[test]
fn toggle_model_disable() {
    let _ = init();

    let all_models = vec!["gpt-4".to_string(), "gpt-3.5".to_string()];

    // Start with both enabled
    save_enabled_models("openai", &all_models).unwrap();

    // Disable gpt-3.5
    toggle_model("openai", "gpt-3.5", false, &all_models).unwrap();

    assert!(is_model_enabled("openai", "gpt-4"));
    assert!(!is_model_enabled("openai", "gpt-3.5"));
}

#[test]
fn list_enabled_models_all() {
    let _ = init();

    let all_models = vec!["gpt-4".to_string(), "gpt-3.5".to_string()];

    // None saved means all enabled
    let enabled = list_enabled_models("openai", &all_models);
    assert_eq!(enabled.len(), 2);
}

#[test]
fn list_enabled_models_specific() {
    let _ = init();

    let all_models = vec![
        "gpt-4".to_string(),
        "gpt-3.5".to_string(),
        "gpt-4-turbo".to_string(),
    ];
    let enabled_models = vec!["gpt-4".to_string(), "gpt-4-turbo".to_string()];

    save_enabled_models("openai", &enabled_models).unwrap();

    let enabled = list_enabled_models("openai", &all_models);
    assert_eq!(enabled.len(), 2);
    assert!(enabled.contains(&"gpt-4".to_string()));
    assert!(enabled.contains(&"gpt-4-turbo".to_string()));
}

#[test]
fn toggle_model_add_new() {
    let _ = init();

    let all_models = vec!["gpt-4".to_string(), "new-model".to_string()];

    // Start with only gpt-4
    save_enabled_models("openai", &["gpt-4".to_string()]).unwrap();

    // Enable new model
    toggle_model("openai", "new-model", true, &all_models).unwrap();

    assert!(is_model_enabled("openai", "gpt-4"));
    assert!(is_model_enabled("openai", "new-model"));
}

#[test]
fn toggle_model_disable_last_one() {
    let _ = init();

    let all_models = vec!["only-model".to_string()];

    // Start with only model enabled
    save_enabled_models("openai", &["only-model".to_string()]).unwrap();

    // Disable it
    toggle_model("openai", "only-model", false, &all_models).unwrap();

    assert!(!is_model_enabled("openai", "only-model"));
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

#[test]
fn enabled_models_empty_list() {
    let _ = init();

    save_enabled_models("openai", &[]).unwrap();

    // Empty list means none enabled
    assert!(!is_model_enabled("openai", "any-model"));
}

#[test]
fn resolve_alias_with_unicode() {
    let _ = init();

    let targets = vec![RouteTarget {
        provider_id: "dashscope".to_string(),
        model_id: "qwen-max".to_string(),
    }];
    set_route_rules("中文别名", targets).unwrap();

    let result = resolve_alias("中文别名").unwrap();
    assert_eq!(result.0, "dashscope");
}
