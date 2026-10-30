use thinoda_platform::settings::{Settings, TreeTableSort};

#[test]
fn local_table_choices_roundtrip_and_legacy_settings_default_safely() {
    let defaults: Settings = toml::from_str("").unwrap();
    assert_eq!(defaults.tree_table.columns, [true; 4]);
    assert_eq!(defaults.tree_table.sort, TreeTableSort::Hierarchy);
    let mut settings = defaults;
    settings.tree_table.columns = [false, true, false, true];
    settings.tree_table.sort = TreeTableSort::Due;
    let encoded = toml::to_string(&settings).unwrap();
    let decoded: Settings = toml::from_str(&encoded).unwrap();
    assert_eq!(decoded.tree_table, settings.tree_table);
    let future: Settings = toml::from_str("[tree_table]\nsort = 'future_sort'\n").unwrap();
    assert_eq!(future.tree_table.sort, TreeTableSort::Hierarchy);
}
