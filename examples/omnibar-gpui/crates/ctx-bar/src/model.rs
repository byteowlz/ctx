use std::collections::HashSet;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

macro_rules! selector {
    ($name:ident, $default:ident, [$($variant:ident => $text:literal),+]) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "lowercase")]
        pub enum $name { $( $variant, )+ }
        impl Default for $name { fn default() -> Self { Self::$default } }
        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub fn label(self) -> &'static str { match self { $(Self::$variant => $text),+ } }
            pub fn next(self) -> Self {
                let index = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
                Self::ALL[(index + 1) % Self::ALL.len()]
            }
        }
    };
}
selector!(Platform, Macos, [Macos => "macOS", Windows => "Windows", Linux => "Linux", Omarchy => "Omarchy"]);
selector!(Fixture, Desktop, [Desktop => "Desktop", Selection => "Selection", Audio => "Audio", Unavailable => "Unavailable"]);
selector!(Presentation, Flat, [Flat => "Flat", Branches => "Branches"]);

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Item {
    pub id: String,
    pub tool: String,
    pub interface: String,
    pub label: String,
    pub description: String,
    pub icon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<Platform>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probability: Option<f64>,
}

impl Item {
    pub fn offered(&self, platform: Platform, fixture: Fixture) -> bool {
        let platform_ok = self
            .platforms
            .as_ref()
            .is_none_or(|p| p.contains(&platform));
        let context_ok = match self.requires.as_deref() {
            None => true,
            Some("window") => fixture != Fixture::Unavailable,
            Some("selection") => fixture == Fixture::Selection,
            Some("audio") => fixture == Fixture::Audio,
            _ => false,
        };
        platform_ok && context_ok
    }

    pub fn is_branch(&self) -> bool {
        self.kind.as_deref() == Some("branch")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Catalog {
    pub items: Vec<Item>,
    #[serde(default)]
    pub branches: Vec<Item>,
}

impl Catalog {
    fn candidates(&self, request: &Request) -> Result<Vec<&Item>> {
        let available: Vec<_> = self
            .items
            .iter()
            .filter(|item| item.offered(request.platform, request.fixture))
            .collect();
        if request.presentation == Presentation::Flat {
            if request.node != "root" {
                bail!("Flat suggestions have no child node")
            }
            return Ok(available);
        }
        if request.node != "root" {
            let branch = self
                .branches
                .iter()
                .find(|branch| branch.node.as_deref() == Some(request.node.as_str()))
                .ok_or_else(|| anyhow::anyhow!("Unknown branch node"))?;
            return Ok(available
                .into_iter()
                .filter(|item| {
                    branch
                        .children
                        .as_ref()
                        .is_some_and(|children| children.contains(&item.id))
                })
                .collect());
        }
        let branches: Vec<_> = self
            .branches
            .iter()
            .filter(|branch| {
                branch.offered(request.platform, request.fixture)
                    && available.iter().any(|item| {
                        branch
                            .children
                            .as_ref()
                            .is_some_and(|children| children.contains(&item.id))
                    })
            })
            .collect();
        let grouped: HashSet<_> = branches
            .iter()
            .flat_map(|branch| branch.children.iter().flatten())
            .collect();
        let ungrouped: Vec<_> = available
            .into_iter()
            .filter(|item| !grouped.contains(&item.id))
            .collect();
        Ok(branches.into_iter().chain(ungrouped).collect())
    }
}

pub fn fixture_catalog() -> Result<Catalog> {
    serde_json::from_str(include_str!("../../../../omnibar-prototype/catalog.json"))
        .map_err(Into::into)
}

#[derive(Debug, Clone, Serialize)]
pub struct Request {
    pub query: String,
    pub platform: Platform,
    pub fixture: Fixture,
    pub presentation: Presentation,
    pub node: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestions {
    pub items: Vec<Item>,
    pub source: String,
    pub latency_ms: f64,
}

/// Validate against prototype fixtures, never treat model confidence as authorization.
/// Catalog data comes from the local proxy; no invented branch IDs are accepted.
pub fn validate(response: &Suggestions, request: &Request, catalog: &Catalog) -> Result<()> {
    if response.items.len() > 5
        || response.source != "Jev via EAVS"
        || !response.latency_ms.is_finite()
        || response.latency_ms < 0.0
    {
        bail!("Invalid proxy response envelope (no fixture inference fallback)");
    }
    let offered = catalog.candidates(request)?;
    let mut seen = HashSet::new();
    for item in &response.items {
        let Some(probability) = item.probability else {
            bail!("Missing suggestion probability")
        };
        if !probability.is_finite() || !(0.0..=1.0).contains(&probability) || !seen.insert(&item.id)
        {
            bail!("Invalid or duplicate suggestion probability");
        }
        if item.is_branch()
            && (request.presentation != Presentation::Branches
                || request.node != "root"
                || item
                    .node
                    .as_deref()
                    .is_none_or(|node| node.is_empty() || node == "root"))
        {
            bail!("Branches are limited to root plus one child level");
        }
        if item
            .kind
            .as_deref()
            .is_some_and(|kind| kind != "branch" && kind != "leaf")
        {
            bail!("Unknown suggestion kind");
        }
        let expected = offered
            .iter()
            .copied()
            .find(|i| i.id == item.id)
            .ok_or_else(|| anyhow::anyhow!("Unlisted suggestion"))?;
        let mut actual = item.clone();
        let mut declaration = expected.clone();
        actual.probability = None;
        declaration.probability = None;
        if actual != declaration {
            bail!("Suggestion does not match the offered prototype catalog");
        }
        if !item.is_branch() && item.node.is_some() {
            bail!("Leaf cannot expand a branch")
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_gating_matches_proxy() {
        let catalog = fixture_catalog().unwrap();
        let find = |id: &str| catalog.items.iter().find(|i| i.id == id).unwrap();
        assert!(!find("screenshot.capture").offered(Platform::Macos, Fixture::Unavailable));
        assert!(find("selection.speak").offered(Platform::Linux, Fixture::Selection));
        assert!(!find("selection.speak").offered(Platform::Linux, Fixture::Audio));
        assert!(find("audio.transcribe").offered(Platform::Windows, Fixture::Audio));
        assert!(!find("theme.pick").offered(Platform::Macos, Fixture::Desktop));
        assert!(find("theme.pick").offered(Platform::Omarchy, Fixture::Desktop));
    }
    #[test]
    fn declared_branch_ids_and_two_level_boundary_are_validated() {
        let mut catalog = fixture_catalog().unwrap();
        let mut branch = catalog.items[0].clone();
        branch.id = "fixture.appearance".into();
        branch.kind = Some("branch".into());
        branch.node = Some("test-appearance".into());
        branch.id = "test.appearance".into();
        branch.children = Some(vec![catalog.items[0].id.clone()]);
        catalog.branches.push(branch.clone());
        branch.probability = Some(0.9);
        let mut req = Request {
            query: "appearance".into(),
            platform: Platform::Macos,
            fixture: Fixture::Desktop,
            presentation: Presentation::Branches,
            node: "root".into(),
        };
        let mut response = Suggestions {
            items: vec![branch],
            source: "Jev via EAVS".into(),
            latency_ms: 1.0,
        };
        assert!(validate(&response, &req, &catalog).is_ok());
        req.presentation = Presentation::Flat;
        assert!(validate(&response, &req, &catalog).is_err());
        req.presentation = Presentation::Branches;
        req.node = "fixture-appearance".into();
        assert!(validate(&response, &req, &catalog).is_err());
        req.node = "root".into();
        response.items[0].node = Some("invented-node".into());
        assert!(validate(&response, &req, &catalog).is_err());
    }
    #[test]
    fn known_leaf_from_another_branch_is_rejected() {
        let catalog = fixture_catalog().unwrap();
        let mut request = Request {
            query: "light mode".into(),
            platform: Platform::Macos,
            fixture: Fixture::Desktop,
            presentation: Presentation::Branches,
            node: "fixture-appearance".into(),
        };
        let mut item = catalog
            .items
            .iter()
            .find(|item| item.id == "sound.volume")
            .unwrap()
            .clone();
        item.probability = Some(0.9);
        let response = Suggestions {
            items: vec![item],
            source: "Jev via EAVS".into(),
            latency_ms: 1.0,
        };
        assert!(validate(&response, &request, &catalog).is_err());
        request.node = "root".into();
        assert!(validate(&response, &request, &catalog).is_err());
        request.presentation = Presentation::Flat;
        assert!(validate(&response, &request, &catalog).is_ok());
        request.node = "not-a-declared-node".into();
        assert!(validate(&response, &request, &catalog).is_err());
    }

    #[test]
    fn malformed_unlisted_and_fake_inference_are_rejected() {
        let catalog = fixture_catalog().unwrap();
        let req = Request {
            query: "open oqto".into(),
            platform: Platform::Macos,
            fixture: Fixture::Desktop,
            presentation: Presentation::Flat,
            node: "root".into(),
        };
        let mut item = catalog
            .items
            .iter()
            .find(|i| i.id == "oqto.open")
            .unwrap()
            .clone();
        item.probability = Some(0.9);
        let mut response = Suggestions {
            items: vec![item],
            source: "Jev via EAVS".into(),
            latency_ms: 200.0,
        };
        assert!(validate(&response, &req, &catalog).is_ok());
        response.items[0].probability = Some(f64::NAN);
        assert!(validate(&response, &req, &catalog).is_err());
        response.items[0].probability = Some(0.9);
        response.items[0].id = "invented.run".into();
        assert!(validate(&response, &req, &catalog).is_err());
        response.items.clear();
        response.source = "demo".into();
        assert!(validate(&response, &req, &catalog).is_err());
    }
}
