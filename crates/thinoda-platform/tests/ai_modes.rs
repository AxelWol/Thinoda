use thinoda_platform::settings::{AiResponseMode, AiSettings};

#[test]
fn legacy_ai_preferences_default_to_balanced_response_guidance() {
    let settings: AiSettings = toml::from_str("model = 'local-chat'\ntemperature = 0.4").unwrap();
    assert_eq!(settings.response_mode, AiResponseMode::Balanced);
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
