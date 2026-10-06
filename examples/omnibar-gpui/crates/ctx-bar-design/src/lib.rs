//! Template-derived design mechanism, with no GPUI types.
//! Studio Lumen snapshots are data, not a visual approval of ctx/omnibar rev1.
pub mod color;
pub mod radius;
pub mod roles;
pub mod scheme;
pub mod theme_set;

pub use radius::RadiusScale;
pub use theme_set::{Identity, theme_set_json};

pub fn studio_theme_set_json() -> anyhow::Result<String> {
    let dark = scheme::example_dark()?;
    let light = scheme::example_light()?;
    let identity = Identity {
        font_family: "SystemUIFont".into(),
        mono_font_family: "SF Mono".into(),
        accent_font_family: "SystemUIFont".into(),
        font_size: 13.5,
        mono_font_size: 12.0,
        radius: RadiusScale::new(10.0),
        shadow: true,
    };
    let mut set: serde_json::Value = serde_json::from_str(&theme_set_json(
        "Ctx Omnibar / Lumen",
        &[&dark, &light],
        &identity,
    )?)?;
    // Native material adaptation: role-derived tint over OS blur. This does not
    // claim Apple Liquid Glass or imply visual approval. No additional slots.
    for (entry, scheme) in set["themes"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .zip([&dark, &light])
    {
        let tint = roles::Roles::new(scheme)
            .get(roles::Role::Background)
            .with_alpha(0.88);
        entry["colors"]["background"] = tint.to_hex().into();
    }
    Ok(serde_json::to_string(&set)?)
}
