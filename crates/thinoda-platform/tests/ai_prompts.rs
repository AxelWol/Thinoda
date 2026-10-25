use thinoda_platform::{
    Paths, Settings,
    ai_prompts::{self, SavedPrompt},
};

fn prompt(id: &str, name: &str) -> SavedPrompt {
    SavedPrompt {
        id: id.into(),
        name: name.into(),
        text: "Suggest next steps.".into(),
    }
}

#[test]
fn library_roundtrips_and_merges_only_its_field_from_fresh_settings() {
    let temp = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(temp.path());
    paths.ensure().unwrap();
    assert!(ai_prompts::load(&paths).unwrap().is_empty());
    let first = vec![prompt("one", "Plan")];
    ai_prompts::save(&paths, &[], &first).unwrap();
    let file = paths.config_dir.join("settings.toml");
    let mut value: toml::Value = toml::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    value["ai"]["base_url"] = "https://changed.example/v1".into();
    value["ai"]["api_key_stored"] = true.into();
    value
        .as_table_mut()
        .unwrap()
        .insert("unknown_future_field".into(), "Preserve this too".into());
    std::fs::write(&file, toml::to_string_pretty(&value).unwrap()).unwrap();
    let second = vec![prompt("one", "Updated")];
    ai_prompts::save(&paths, &first, &second).unwrap();
    assert_eq!(ai_prompts::load(&paths).unwrap(), second);
    let text = std::fs::read_to_string(&file).unwrap();
    let current: toml::Value = toml::from_str(&text).unwrap();
    assert_eq!(current["ai"]["base_url"], value["ai"]["base_url"]);
    assert_eq!(
        current["ai"]["api_key_stored"],
        value["ai"]["api_key_stored"]
    );
    assert_eq!(
        current["unknown_future_field"],
        value["unknown_future_field"]
    );
    assert!(current["ai"].get("api_key").is_none());
    let settings: Settings = toml::from_str(&text).unwrap();
    assert_eq!(settings.ai.saved_prompts, second);
    assert!(!format!("{:?}", settings.ai).contains("Suggest next steps."));
}

#[test]
fn conflicts_corrupt_settings_and_legacy_secrets_are_not_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(temp.path());
    paths.ensure().unwrap();
    let prompts = vec![prompt("one", "Plan")];
    ai_prompts::save(&paths, &[], &prompts).unwrap();
    let file = paths.config_dir.join("settings.toml");
    let before = std::fs::read(&file).unwrap();
    assert!(ai_prompts::save(&paths, &[], &[]).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), before);
    for text in [
        "broken = [",
        "[ai]\napi_key = 'legacy-must-not-be-rewritten'",
    ] {
        std::fs::write(&file, text).unwrap();
        assert!(ai_prompts::save(&paths, &[], &prompts).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
    }
}

#[test]
fn library_validation_enforces_names_ids_utf8_and_all_byte_bounds() {
    let mut value = prompt("one", &"n".repeat(ai_prompts::MAX_NAME_BYTES));
    value.text = "x".repeat(ai_prompts::MAX_TEXT_BYTES);
    assert!(ai_prompts::validate(&[value.clone()]).is_ok());
    value.name.push('é');
    assert!(ai_prompts::validate(&[value.clone()]).is_err());
    value.name = "Valid".into();
    value.text.push('é');
    assert!(ai_prompts::validate(&[value]).is_err());
    assert!(ai_prompts::validate(&[prompt("one", "Plan"), prompt("two", " plan ")]).is_err());
    assert!(ai_prompts::validate(&[prompt("one", "Plan"), prompt("one", "Other")]).is_err());
    assert!(ai_prompts::validate(&[prompt("../bad", "Plan")]).is_err());
    let many: Vec<_> = (0..=ai_prompts::MAX_PROMPTS)
        .map(|n| prompt(&format!("id{n}"), &format!("Name {n}")))
        .collect();
    assert!(ai_prompts::validate(&many).is_err());
    let huge: Vec<_> = (0..ai_prompts::MAX_PROMPTS)
        .map(|n| SavedPrompt {
            text: "x".repeat(ai_prompts::MAX_TEXT_BYTES),
            ..prompt(&format!("id{n}"), &format!("Name {n}"))
        })
        .collect();
    assert!(ai_prompts::validate(&huge).is_err());
}

#[test]
fn persistence_errors_leave_the_original_library_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(temp.path());
    paths.ensure().unwrap();
    let first = vec![prompt("one", "Plan")];
    ai_prompts::save(&paths, &[], &first).unwrap();
    let file = paths.config_dir.join("settings.toml");
    let bytes = std::fs::read(&file).unwrap();
    let invalid = vec![prompt("two", "")];
    assert!(ai_prompts::save(&paths, &first, &invalid).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), bytes);
}
