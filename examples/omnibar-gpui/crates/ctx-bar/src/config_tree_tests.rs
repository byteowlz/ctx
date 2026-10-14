use super::*;

#[test]
fn tree_view_and_variants_are_canonical_typed_settings() {
    for view in ["list", "tree"] {
        for variant in ["faint", "trail", "crumbs"] {
            let cfg: Config = config::Config::builder()
                .add_source(config::File::from_str(
                    &format!("view={view:?}\ntree_variant={variant:?}\nreduce_motion=true"),
                    config::FileFormat::Toml,
                ))
                .build()
                .unwrap()
                .try_deserialize()
                .unwrap();
            assert_eq!(serde_json::to_value(cfg.view).unwrap(), view);
            assert_eq!(serde_json::to_value(cfg.tree_variant).unwrap(), variant);
            assert!(cfg.reduce_motion);
        }
    }
    for text in [
        "view='Tree'",
        "tree_variant='breadcrumbs'",
        "view=false",
        "tree_variant=3",
        "reduce_motion='not-a-bool'",
        "tree_mode='tree'",
    ] {
        assert!(
            config::Config::builder()
                .add_source(config::File::from_str(text, config::FileFormat::Toml))
                .build()
                .unwrap()
                .try_deserialize::<Config>()
                .is_err(),
            "{text}"
        );
    }
}
