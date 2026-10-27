use thinoda_platform::settings::{AiProvider, AiResponseMode, AiSettings};

#[test]
fn legacy_ai_preferences_default_to_balanced_response_guidance() {
    let settings: AiSettings = toml::from_str("model = 'local-chat'\ntemperature = 0.4").unwrap();
    assert_eq!(settings.response_mode, AiResponseMode::Balanced);
    assert_eq!(settings.provider, AiProvider::OpenAiApi);
}

#[test]
fn provider_switches_roundtrip_and_preserve_api_preferences() {
    let mut settings = AiSettings {
        base_url: "https://example.invalid/v1".into(),
        model: "api-model".into(),
        embedding_model: "embedding-model".into(),
        cli_models: [
            "copilot-model".into(),
            "claude-model".into(),
            "codex-model".into(),
        ],
        api_key: "not-for-settings-toml".into(),
        ..Default::default()
    };
    for provider in AiProvider::ALL {
        settings.provider = provider;
        let text = toml::to_string(&settings).unwrap();
        assert!(!text.contains("not-for-settings-toml"));
        let restored: AiSettings = toml::from_str(&text).unwrap();
        assert_eq!(restored.provider, provider);
        assert_eq!(restored.base_url, settings.base_url);
        assert_eq!(restored.model, settings.model);
        assert_eq!(restored.embedding_model, settings.embedding_model);
        assert_eq!(restored.cli_models, settings.cli_models);
        assert!(restored.api_key.is_empty());
        assert_eq!(settings.api_key, "not-for-settings-toml");
    }
}

#[test]
fn chat_response_modes_roundtrip_without_persisting_credentials() {
    for mode in [
        AiResponseMode::Quick,
        AiResponseMode::Balanced,
        AiResponseMode::Deep,
    ] {
        let settings = AiSettings {
            response_mode: mode,
            api_key: "not-for-settings-toml".into(),
            ..AiSettings::default()
        };
        let text = toml::to_string(&settings).unwrap();
        assert!(!text.contains("not-for-settings-toml"));
        let restored: AiSettings = toml::from_str(&text).unwrap();
        assert_eq!(restored.response_mode, mode);
        assert!(restored.api_key.is_empty());
    }
}
