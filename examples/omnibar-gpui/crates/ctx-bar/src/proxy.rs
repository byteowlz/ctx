//! Only the loopback prototype proxy is reachable. No EAVS/provider credentials,
//! desktop capture, command invocation, redirect following, or inference fallback.
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::model::{Catalog, Request, Suggestions, validate};

const ORIGIN: &str = "http://127.0.0.1:4784";
const MAX_RESPONSE: u64 = 64 * 1024;

// Deliberately no Debug/Serialize: reviewKey is an ephemeral proxy capability.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Descriptor {
    origin: String,
    review_key: String,
}
impl Descriptor {
    pub fn read(path: &Path) -> Result<Self> {
        let file = File::open(path)
            .context("Start the local prototype proxy; review descriptor unavailable")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            if file.metadata()?.permissions().mode() & 0o077 != 0 {
                bail!("Review descriptor must be private (owner-only permissions)")
            }
        }
        let mut bytes = Vec::new();
        file.take(4097).read_to_end(&mut bytes)?;
        if bytes.len() > 4096 {
            bail!("Invalid review descriptor")
        }
        let descriptor: Self = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("Invalid review descriptor"))?;
        descriptor.validate()?;
        Ok(descriptor)
    }
    fn validate(&self) -> Result<()> {
        if self.origin != ORIGIN
            || self.review_key.len() < 16
            || self.review_key.len() > 256
            || !self.review_key.bytes().all(|b| b.is_ascii_graphic())
        {
            bail!("Review descriptor must point to the loopback prototype proxy")
        }
        Ok(())
    }
}

pub struct Proxy {
    client: Client,
    descriptor: Descriptor,
}
impl Proxy {
    pub fn new(path: &Path, timeout_seconds: u64) -> Result<Self> {
        let descriptor = Descriptor::read(path)?;
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .map_err(|_| anyhow::anyhow!("Cannot initialize local proxy transport"))?;
        Ok(Self { client, descriptor })
    }
    fn response<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> Result<T> {
        let mut review_key = reqwest::header::HeaderValue::from_str(&self.descriptor.review_key)
            .map_err(|_| anyhow::anyhow!("Invalid review credential"))?;
        review_key.set_sensitive(true);
        let response = request
            .header("Origin", &self.descriptor.origin)
            .header("X-Prototype-Key", review_key)
            .send()
            .map_err(|_| {
                anyhow::anyhow!(
                    "Local Jev proxy unavailable or timed out; retry after starting the proxy"
                )
            })?;
        if !response.status().is_success() {
            // Do not surface upstream bodies, headers, URLs, keys or private diagnostics.
            bail!(
                "Local Jev proxy returned HTTP {}; no synthetic inference substituted",
                response.status().as_u16()
            )
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_RESPONSE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| anyhow::anyhow!("Could not read local proxy response"))?;
        if bytes.len() as u64 > MAX_RESPONSE {
            bail!("Local proxy response too large")
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("Local proxy returned an invalid JSON shape"))
    }
    pub fn catalog(&self) -> Result<Catalog> {
        let catalog: Catalog = self.response(self.client.get(format!("{ORIGIN}/api/catalog")))?;
        if catalog.items.len() + catalog.branches.len() > 128 {
            bail!("Prototype catalog too large")
        }
        Ok(catalog)
    }
    pub fn suggest(&self, request: &Request, catalog: &Catalog) -> Result<Suggestions> {
        if request.query.trim().is_empty() || request.query.chars().count() > 512 {
            bail!("Enter 1–512 characters")
        }
        let result = self.response(
            self.client
                .post(format!("{ORIGIN}/api/suggest"))
                .json(request),
        )?;
        validate(&result, request, catalog)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Ephemeral local mocks only. These tests never bind the live proxy port or
    // load the real review descriptor and never contact EAVS.
    fn mock_reply(status: &str, body: String) -> (String, std::thread::JoinHandle<String>) {
        use std::io::Write as _;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/api/suggest", listener.local_addr().unwrap());
        let status = status.to_string();
        let thread = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut received = Vec::new();
            let mut buffer = [0u8; 2048];
            loop {
                let n = socket.read(&mut buffer).unwrap();
                if n == 0 {
                    break;
                }
                received.extend_from_slice(&buffer[..n]);
                if let Some(header_end) = received.windows(4).position(|v| v == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&received[..header_end]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|s| s.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if received.len() >= header_end + 4 + length {
                        break;
                    }
                }
            }
            let reply = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            // Oversize/error tests may close the socket before all body bytes are read.
            let _ = socket.write_all(reply.as_bytes());
            String::from_utf8(received).unwrap()
        });
        (url, thread)
    }
    fn mock_proxy() -> Proxy {
        Proxy {
            client: Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap(),
            descriptor: Descriptor {
                origin: ORIGIN.into(),
                review_key: "synthetic-review-capability-only".into(),
            },
        }
    }
    #[test]
    fn mock_http_headers_payload_and_decision_shape() {
        use crate::model::{Fixture, Platform, Presentation, fixture_catalog};
        let (url, thread) = mock_reply(
            "200 OK",
            r#"{"items":[],"source":"Jev via EAVS","latencyMs":23}"#.into(),
        );
        let proxy = mock_proxy();
        let request = Request {
            query: "synthetic no match".into(),
            platform: Platform::Macos,
            fixture: Fixture::Unavailable,
            presentation: Presentation::Flat,
            node: "root".into(),
        };
        let response: Suggestions = proxy
            .response(proxy.client.post(url).json(&request))
            .unwrap();
        validate(&response, &request, &fixture_catalog().unwrap()).unwrap();
        let received = thread.join().unwrap();
        let lower = received.to_ascii_lowercase();
        assert!(lower.starts_with("post /api/suggest "));
        assert!(lower.contains("origin: http://127.0.0.1:4784"));
        assert!(lower.contains("x-prototype-key: synthetic-review-capability-only"));
        assert!(!lower.contains("authorization:"));
        let body: serde_json::Value =
            serde_json::from_str(received.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["query"], "synthetic no match");
        assert_eq!(body["fixture"], "unavailable");
        assert_eq!(body["node"], "root");
        assert!(body.get("context").is_none());
    }
    #[test]
    fn upstream_body_is_withheld_and_oversize_response_rejected() {
        for (status, body, expected) in [
            (
                "503 Unavailable",
                "synthetic-private-upstream-detail".into(),
                "HTTP 503",
            ),
            ("200 OK", "x".repeat(MAX_RESPONSE as usize + 1), "too large"),
            (
                "200 OK",
                "synthetic-private-upstream-detail".into(),
                "invalid JSON shape",
            ),
        ] {
            let (url, thread) = mock_reply(status, body);
            let proxy = mock_proxy();
            let error = proxy
                .response::<Suggestions>(proxy.client.post(url))
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected));
            assert!(!error.contains("synthetic-private-upstream-detail"));
            thread.join().unwrap();
        }
    }
    #[test]
    fn descriptor_allows_only_fixed_loopback_origin_and_keeps_credentials_separate() {
        let key = "synthetic-review-capability-only";
        let mut d = Descriptor {
            origin: ORIGIN.into(),
            review_key: key.into(),
        };
        d.validate().unwrap();
        for origin in [
            "http://localhost:4784",
            "http://127.0.0.1:3033",
            "https://example.com",
            "http://127.0.0.1:4784/anything",
        ] {
            d.origin = origin.into();
            assert!(d.validate().is_err());
        }
        assert!(serde_json::from_str::<Descriptor>(r#"{"origin":"http://127.0.0.1:4784","reviewKey":"synthetic-review-capability-only","apiKey":"forbidden"}"#).is_err());
    }
}
