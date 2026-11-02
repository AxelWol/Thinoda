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
        ollama_base_url: "http://localhost:11434/v1".into(),
        ollama_model: "local-chat".into(),
        ollama_embedding_model: "nomic-embed-text".into(),
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
        assert_eq!(restored.ollama_base_url, settings.ollama_base_url);
        assert_eq!(restored.ollama_model, settings.ollama_model);
        assert_eq!(
            restored.ollama_embedding_model,
            settings.ollama_embedding_model
        );
        assert!(restored.api_key.is_empty());
        assert_eq!(settings.api_key, "not-for-settings-toml");
    }
}

#[test]
fn generation_configuration_rejects_missing_and_invalid_endpoints() {
    let mut settings = AiSettings::default();
    assert!(settings.generation_configured()); // blank model is automatic
    for endpoint in [
        "",
        "not a URL",
        "file:///tmp/api",
        "http://",
        "http://user:password@localhost/v1",
        "http://localhost/v1?token=secret",
        "https://localhost/v1#fragment",
    ] {
        settings.base_url = endpoint.into();
        assert!(!settings.generation_configured(), "{endpoint}");
    }
    settings.provider = AiProvider::Ollama;
    settings.credentials_ready = false;
    assert!(settings.generation_configured());
    settings.ollama_base_url.clear();
    assert!(!settings.generation_configured());
    settings.provider = AiProvider::None;
    assert!(!settings.generation_configured());
    settings.provider = AiProvider::ClaudeCli;
    assert!(settings.generation_configured());
    settings.enabled = false;
    assert!(!settings.generation_configured());
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
