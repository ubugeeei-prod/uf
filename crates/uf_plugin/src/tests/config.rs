//! What a user can write in `plugins: [...]`, read back through `uf.config.js`.

use uf_config::{
    ApplyCondition, HookOrder, PluginEntry, PluginSpec, UniflowedConfig, extract_config_object,
};

fn parse(source: &str) -> UniflowedConfig {
    let object = extract_config_object(source).expect("a config object");
    json5::from_str(&object).expect("a config")
}

#[test]
fn a_project_with_no_plugins_field_has_no_plugins() {
    assert!(parse("export default defineConfig({});").plugins.is_empty());
}

#[test]
fn an_empty_list_is_no_plugins() {
    assert!(
        parse("export default defineConfig({ plugins: [] });")
            .plugins
            .is_empty()
    );
}

#[test]
fn a_string_entry_takes_the_defaults() {
    let config = parse(r#"export default defineConfig({ plugins: ["@uniflowed/plugin-mdx"] });"#);

    assert_eq!(
        config.plugins,
        vec![PluginEntry::Name("@uniflowed/plugin-mdx".into())]
    );
    assert_eq!(config.plugins[0].name(), "@uniflowed/plugin-mdx");
    assert_eq!(config.plugins[0].order(), HookOrder::Normal);
    assert_eq!(config.plugins[0].apply(), ApplyCondition::Always);
}

#[test]
fn an_object_entry_carries_its_order_and_apply() {
    let config = parse(
        r#"
export default defineConfig({
  plugins: [{ name: "./plugins/metrics.js", order: "post", apply: "build" }],
});
"#,
    );

    assert_eq!(
        config.plugins,
        vec![PluginEntry::Spec(
            PluginSpec::new("./plugins/metrics.js")
                .with_order(HookOrder::Post)
                .with_apply(ApplyCondition::Build)
        )]
    );
    assert_eq!(config.plugins[0].order(), HookOrder::Post);
    assert_eq!(config.plugins[0].apply(), ApplyCondition::Build);
}

#[test]
fn an_object_entry_may_leave_both_knobs_out() {
    let config = parse(r#"export default defineConfig({ plugins: [{ name: "mdx" }] });"#);

    assert_eq!(config.plugins[0].name(), "mdx");
    assert_eq!(config.plugins[0].order(), HookOrder::Normal);
    assert_eq!(config.plugins[0].apply(), ApplyCondition::Always);
}

#[test]
fn the_two_spellings_mix_in_one_list() {
    let config = parse(
        r#"
export default defineConfig({
  plugins: [
    "a",
    { name: "b", order: "pre" },
    "c",
    { name: "d", apply: "serve" },
  ],
});
"#,
    );

    assert_eq!(
        config
            .plugins
            .iter()
            .map(|entry| (entry.name(), entry.order(), entry.apply()))
            .collect::<Vec<_>>(),
        vec![
            ("a", HookOrder::Normal, ApplyCondition::Always),
            ("b", HookOrder::Pre, ApplyCondition::Always),
            ("c", HookOrder::Normal, ApplyCondition::Always),
            ("d", HookOrder::Normal, ApplyCondition::Serve),
        ]
    );
}

#[test]
fn every_order_spelling_parses() {
    for (value, expected) in [
        ("pre", HookOrder::Pre),
        ("normal", HookOrder::Normal),
        ("post", HookOrder::Post),
    ] {
        let config = parse(&format!(
            r#"export default defineConfig({{ plugins: [{{ name: "a", order: "{value}" }}] }});"#
        ));

        assert_eq!(config.plugins[0].order(), expected, "{value}");
    }
}

#[test]
fn every_apply_spelling_parses() {
    for (value, expected) in [
        ("build", ApplyCondition::Build),
        ("serve", ApplyCondition::Serve),
        ("always", ApplyCondition::Always),
    ] {
        let config = parse(&format!(
            r#"export default defineConfig({{ plugins: [{{ name: "a", apply: "{value}" }}] }});"#
        ));

        assert_eq!(config.plugins[0].apply(), expected, "{value}");
    }
}

#[test]
fn an_unknown_order_is_refused() {
    let object = extract_config_object(
        r#"export default defineConfig({ plugins: [{ name: "a", order: "first" }] });"#,
    )
    .expect("a config object");

    assert!(json5::from_str::<UniflowedConfig>(&object).is_err());
}

#[test]
fn an_unknown_apply_condition_is_refused() {
    let object = extract_config_object(
        r#"export default defineConfig({ plugins: [{ name: "a", apply: "sometimes" }] });"#,
    )
    .expect("a config object");

    assert!(json5::from_str::<UniflowedConfig>(&object).is_err());
}

#[test]
fn plugins_round_trip_through_json() {
    let config = parse(
        r#"
export default defineConfig({
  plugins: ["a", { name: "b", order: "pre", apply: "serve" }],
});
"#,
    );

    let json = serde_json::to_string(&config.plugins).expect("serializes");
    let back: Vec<PluginEntry> = serde_json::from_str(&json).expect("deserializes");

    assert_eq!(back, config.plugins);
}

#[test]
fn a_spec_converts_into_an_entry() {
    let spec = PluginSpec::new("a")
        .with_order(HookOrder::Pre)
        .with_apply(ApplyCondition::Build);

    assert_eq!(PluginEntry::from(spec.clone()), PluginEntry::Spec(spec));
}

#[test]
fn a_spec_builder_only_changes_what_it_is_asked_to() {
    let spec = PluginSpec::new("a");

    assert_eq!(spec.name, "a");
    assert_eq!(spec.order, HookOrder::Normal);
    assert_eq!(spec.apply, ApplyCondition::Always);
    assert_eq!(spec.clone().with_order(HookOrder::Post).name, "a");
    assert_eq!(
        spec.with_apply(ApplyCondition::Serve).order,
        HookOrder::Normal
    );
}

#[test]
fn a_spec_defaults_to_an_unnamed_normal_always_plugin() {
    let spec = PluginSpec::default();

    assert!(spec.name.is_empty());
    assert_eq!(spec.order, HookOrder::Normal);
    assert_eq!(spec.apply, ApplyCondition::Always);
}

#[test]
fn a_plugin_name_with_a_traversal_survives_parsing_and_is_caught_at_resolution() {
    // Parsing is not the guard: the config layer stores whatever was written,
    // and `uf_plugin` is the single place that decides whether it is usable.
    let config = parse(r#"export default defineConfig({ plugins: ["../../evil.js"] });"#);

    assert_eq!(config.plugins[0].name(), "../../evil.js");
    assert!(
        crate::classify_plugin_name("../../evil.js", camino::Utf8Path::new("/workspace")).is_err()
    );
}
