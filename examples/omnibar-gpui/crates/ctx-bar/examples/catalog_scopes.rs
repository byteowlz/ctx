//! JSON parity evidence for declarations/scopes; does not access host or provider.
use anyhow::Result;
use ctx_bar::model::{Fixture, Platform, Presentation, Request, fixture_catalog};

fn main() -> Result<()> {
    let catalog = fixture_catalog()?;
    let mut scopes = Vec::new();
    for platform in Platform::ALL {
        for fixture in Fixture::ALL {
            for presentation in Presentation::ALL {
                let nodes: Vec<_> = if *presentation == Presentation::Branches {
                    std::iter::once("root".to_owned())
                        .chain(
                            catalog
                                .branches
                                .iter()
                                .filter_map(|branch| branch.node.clone()),
                        )
                        .collect()
                } else {
                    vec!["root".to_owned()]
                };
                for node in nodes {
                    let request = Request {
                        query: "synthetic scope proof".into(),
                        platform: *platform,
                        fixture: *fixture,
                        presentation: *presentation,
                        node,
                    };
                    let ids: Vec<_> = catalog
                        .candidates(&request)?
                        .iter()
                        .map(|item| item.id.as_str())
                        .collect();
                    scopes.push(serde_json::json!({"platform":platform,"fixture":fixture,"presentation":presentation,"node":request.node,"ids":ids}));
                }
            }
        }
    }
    println!("{}", serde_json::to_string(&scopes)?);
    Ok(())
}
