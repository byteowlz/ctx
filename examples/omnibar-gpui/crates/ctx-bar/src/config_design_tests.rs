//! Native design configuration contract: defaults/schema parity, canonical
//! selectors, expansion geometry, and environment/CLI precedence.
use super::*;

#[test]
fn default_toml_matches_typed_defaults_and_schema() {
    let cfg: Config = config::Config::builder()
        .add_source(config::File::from_str(
            DEFAULT_TOML,
            config::FileFormat::Toml,
        ))
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();
    cfg.validate().unwrap();
    let values = serde_json::to_value(&cfg).unwrap();
    assert_eq!(values, serde_json::to_value(Config::default()).unwrap());
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../examples/omnibar-gpui.schema.json")).unwrap();
    for (key, value) in values.as_object().unwrap() {
        if key != "platform" {
            // Build-OS default is intentionally not fixed in the schema.
            let schema_default = &schema["properties"][key]["default"];
            if value.is_number() {
                assert_eq!(schema_default.as_f64(), value.as_f64(), "{key}");
            } else {
                assert_eq!(schema_default, value, "{key}");
            }
        }
    }
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["height"]["minimum"], 240);
    assert_eq!(schema["properties"]["height"]["maximum"], 900);
    assert_eq!(
        schema["properties"]["design"]["enum"],
        serde_json::json!(BarDesign::ALL.map(BarDesign::id))
    );
    assert_eq!(
        schema["properties"]["presentation"]["enum"],
        serde_json::json!(["flat", "branches"])
    );
}
fn from_toml(text: &str) -> Result<Config> {
    Ok(config::Config::builder()
        .add_source(config::File::from_str(text, config::FileFormat::Toml))
        .build()?
        .try_deserialize()?)
}
#[test]
fn all_designs_and_presentations_parse_but_aliases_and_unknown_fields_fail() {
    let old = from_toml("height = 520.0").unwrap();
    assert_eq!(old.design, BarDesign::DotMatrix);
    assert_eq!(old.presentation, Presentation::Flat);
    old.validate().unwrap();
    for design in BarDesign::ALL {
        for presentation in ["flat", "branches"] {
            let cfg = from_toml(&format!(
                "design = {:?}\npresentation = {presentation:?}",
                design.id()
            ))
            .unwrap();
            assert_eq!(cfg.design, design);
            assert_eq!(
                serde_json::to_value(cfg.presentation).unwrap(),
                presentation
            );
            cfg.validate().unwrap();
        }
    }
    for invalid in ["dot", "DotMatrix", "dot_matrix", "DOT-MATRIX", "unknown"] {
        assert!(
            from_toml(&format!("design = {invalid:?}")).is_err(),
            "{invalid}"
        );
    }
    for invalid in ["branch", "Flat", "unknown"] {
        assert!(from_toml(&format!("presentation = {invalid:?}")).is_err());
    }
    assert!(from_toml("unknown_field = true").is_err());
    assert!(from_toml("design = 1").is_err());
    assert!(from_toml("presentation = false").is_err());
}
#[test]
fn expansion_height_accepts_legacy_and_new_minimum_not_idle_geometry() {
    for height in [240.0, 520.0, 900.0] {
        Config {
            height,
            ..Config::default()
        }
        .validate()
        .unwrap();
    }
    for height in [56.0, 64.0, 239.0, 901.0, f32::NAN, f32::INFINITY] {
        assert!(
            Config {
                height,
                ..Config::default()
            }
            .validate()
            .is_err()
        );
    }
}
#[test]
fn environment_and_cli_design_precedence_without_process_global_mutation() {
    const CHILD_DIR: &str = "CTX_BAR_TEST_THEME_DIR";
    if let Some(dir) = std::env::var_os(CHILD_DIR) {
        let dir = PathBuf::from(dir);
        let global = dir.join("global.toml");
        let local = dir.join("local.toml");
        let explicit = dir.join("explicit.toml");
        let env_cfg = load_layers(&global, Some(&local), None, &[]).unwrap();
        assert_eq!(env_cfg.design, BarDesign::Slot);
        assert_eq!(env_cfg.presentation, Presentation::Branches);
        let file_cfg = load_layers(&global, Some(&local), Some(&explicit), &[]).unwrap();
        assert_eq!(file_cfg.design, BarDesign::Prompt);
        assert_eq!(file_cfg.presentation, Presentation::Flat);
        for design in BarDesign::ALL {
            for presentation in ["flat", "branches"] {
                let args = [
                    "--config",
                    explicit.to_str().unwrap(),
                    "--design",
                    design.id(),
                    "--presentation",
                    presentation,
                ];
                let cfg = load_or_create(args.map(String::from)).unwrap();
                assert_eq!(cfg.design, design);
                assert_eq!(
                    serde_json::to_value(cfg.presentation).unwrap(),
                    presentation
                );
            }
        }
        for args in [
            vec!["--design"],
            vec!["--presentation"],
            vec!["--design", "dot"],
            vec!["--design", "unknown"],
            vec!["--presentation", "branch"],
        ] {
            let mut with_config = vec!["--config", explicit.to_str().unwrap()];
            with_config.extend(args);
            assert!(load_or_create(with_config.into_iter().map(String::from)).is_err());
        }
        assert!(!dir.join("ctx/omnibar-gpui.toml").exists());
        return;
    }
    let dir = std::env::temp_dir().join(format!("ctx-bar-theme-config-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("global.toml"),
        "design = 'underline'\npresentation = 'flat'\n",
    )
    .unwrap();
    fs::write(
        dir.join("local.toml"),
        "design = 'lens'\npresentation = 'flat'\n",
    )
    .unwrap();
    fs::write(
        dir.join("explicit.toml"),
        "design = 'prompt'\npresentation = 'flat'\n",
    )
    .unwrap();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("CTX_BAR__") {
            command.env_remove(key);
        }
    }
    let output = command
        .args(["--exact", "config::design_tests::environment_and_cli_design_precedence_without_process_global_mutation", "--nocapture"])
        .env(CHILD_DIR, &dir)
        .env("XDG_CONFIG_HOME", &dir)
        .env("CTX_BAR__DESIGN", "slot")
        .env("CTX_BAR__PRESENTATION", "branches")
        .output().unwrap();
    fs::remove_dir_all(&dir).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}
