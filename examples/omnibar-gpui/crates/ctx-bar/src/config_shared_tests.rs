use super::*;

#[test]
fn shared_config_migration_preserves_tables_comments_and_legacy_backup() -> Result<()> {
    let dir = std::env::temp_dir().join(format!("ctx-shared-migration-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    let path = dir.join("config.toml");
    let legacy = dir.join("omnibar-gpui.toml");
    let original = "# keep this\n[capture]\nimage_quality = 42\n[eavs]\nmodel = 'jev-latest'\n";
    let old = "theme = 'Lumen Light'\ndesign = 'corners'\nwidth = 900.0\n";
    fs::write(&path, original)?;
    fs::write(&legacy, old)?;
    create_default(&path)?;
    let saved = fs::read_to_string(&path)?;
    assert!(saved.starts_with(original));
    assert_eq!(fs::read_to_string(&legacy)?, old);
    let cfg = load_layers(&path, None, None, &[])?;
    assert_eq!(cfg.design, BarDesign::Corners);
    assert_eq!(cfg.width, 900.0);
    assert_eq!(cfg.theme, "Lumen Light");
    create_default(&path)?;
    assert_eq!(fs::read_to_string(&path)?, saved);
    fs::write(&legacy, "broken = [")?;
    create_default(&path)?; // A current table always wins; legacy is no longer consulted.
    assert_eq!(fs::read_to_string(&path)?, saved);
    fs::write(&path, original)?;
    assert!(create_default(&path).is_err());
    assert_eq!(fs::read_to_string(&path)?, original);
    fs::write(&path, "[bar]\nunknown = true\n")?;
    assert!(load_layers(&path, None, None, &[]).is_err());
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn native_first_run_creates_only_shared_sections() -> Result<()> {
    let dir = std::env::temp_dir().join(format!("ctx-shared-first-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    let path = dir.join("config.toml");
    create_default(&path)?;
    let cfg = load_layers(&path, None, None, &[])?;
    assert_eq!(
        serde_json::to_value(cfg)?,
        serde_json::to_value(Config::default())?
    );
    let saved: serde_json::Value = config::Config::builder()
        .add_source(config::File::from(path))
        .build()?
        .try_deserialize()?;
    assert!(saved.get("bar").is_some());
    assert!(saved.get("design").is_none());
    assert!(dir.join("config.schema.json").exists());
    assert!(!dir.join("omnibar-gpui.toml").exists());
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn macos_native_first_migrates_core_config_without_global_env_mutation() -> Result<()> {
    const CHILD: &str = "CTX_BAR_TEST_SHARED_HOME";
    if let Some(home) = std::env::var_os(CHILD) {
        let home = PathBuf::from(home);
        let path = config_path()?;
        assert_eq!(path, home.join(".config/ctx/config.toml"));
        create_default(&path)?;
        let text = fs::read_to_string(&path)?;
        assert!(text.starts_with("# old core\n[capture]\nimage_quality = 43\n"));
        assert_eq!(
            load_layers(&path, None, None, &[])?.design,
            BarDesign::DotMatrix
        );
        fs::write(
            home.join("Library/Application Support/ctx/config.toml"),
            "broken = [",
        )?;
        create_default(&path)?;
        assert_eq!(fs::read_to_string(path)?, text);
        return Ok(());
    }
    let home =
        std::env::temp_dir().join(format!("ctx-native-mac-migration-{}", std::process::id()));
    let legacy = home.join("Library/Application Support/ctx/config.toml");
    fs::create_dir_all(legacy.parent().unwrap())?;
    fs::write(legacy, "# old core\n[capture]\nimage_quality = 43\n")?;
    let output = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "config::shared_tests::macos_native_first_migrates_core_config_without_global_env_mutation", "--nocapture"])
        .env(CHILD, &home).env("HOME", &home).env_remove("XDG_CONFIG_HOME").output()?;
    fs::remove_dir_all(home)?;
    anyhow::ensure!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
