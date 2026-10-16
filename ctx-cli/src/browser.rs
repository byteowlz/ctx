//! Explicit, read-only consumer of brwsr's browser bridge. Never updates OS focus.
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Args, Subcommand};
use serde_json::{Value, json};

const MAX_REPLY: u64 = 256 * 1024;

#[derive(Debug, Args)]
pub struct BrowserCli {
    /// Private owner-only Unix socket of the brwsr bridge
    #[arg(long)]
    socket: Option<PathBuf>,
    /// Bounded read/write timeout; never silently retries
    #[arg(long, default_value_t = 2000, value_parser = clap::value_parser!(u64).range(100..=30000))]
    timeout_ms: u64,
    #[command(subcommand)]
    action: BrowserAction,
}

#[derive(Debug, Subcommand)]
enum BrowserAction {
    /// List only the tabs explicitly granted by the browser owner
    Tabs,
    /// Bounded page text/selection from a specific, consented document
    Read {
        #[arg(long)]
        tab_id: u64,
        #[arg(long)]
        document_id: String,
        #[arg(long, default_value_t = 4096, value_parser = clap::value_parser!(u64).range(1..=16384))]
        max_chars: u64,
    },
}

pub fn run(cli: &BrowserCli) -> anyhow::Result<()> {
    let request = request(&cli.action);
    let result = socket_path(cli.socket.as_deref())
        .and_then(|path| exchange(&path, &request, cli.timeout_ms))
        .unwrap_or_else(|_| {
            json!({
                "version": 1, "id": request["id"], "outcome": "unavailable",
                "code": "browser_bridge_unavailable"
            })
        });
    println!("{}", serde_json::to_string(&result)?);
    match result["outcome"].as_str() {
        Some("ok" | "truncated") => Ok(()),
        _ => anyhow::bail!("browser capture did not complete; see JSON outcome"),
    }
}

fn request(action: &BrowserAction) -> Value {
    let id = format!(
        "ctx-{}-{}",
        std::process::id(),
        time::OffsetDateTime::now_utc().unix_timestamp_nanos()
    );
    match action {
        BrowserAction::Tabs => json!({"version": 1, "id": id, "op": "tabs"}),
        BrowserAction::Read {
            tab_id,
            document_id,
            max_chars,
        } => json!({
            "version": 1, "id": id, "op": "read", "tab_id": tab_id,
            "document_id": document_id, "max_chars": max_chars
        }),
    }
}

fn socket_path(explicit: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    if let Some(base) = std::env::var_os("XDG_STATE_HOME") {
        return Ok(PathBuf::from(base).join("brwsr/bridge/bridge.sock"));
    }
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("home unavailable"))?;
    Ok(PathBuf::from(home).join(".local/state/brwsr/bridge/bridge.sock"))
}

#[cfg(unix)]
fn exchange(path: &Path, request: &Value, timeout_ms: u64) -> anyhow::Result<Value> {
    use std::os::unix::net::UnixStream;
    let mut stream = UnixStream::connect(path)?;
    let timeout = Some(Duration::from_millis(timeout_ms));
    stream.set_read_timeout(timeout)?;
    stream.set_write_timeout(timeout)?;
    stream.write_all(b"{\"role\":\"client\"}\n")?;
    serde_json::to_writer(&mut stream, request)?;
    stream.write_all(b"\n")?;
    let mut reply = Vec::new();
    BufReader::new(stream)
        .take(MAX_REPLY + 1)
        .read_until(b'\n', &mut reply)?;
    if reply.len() as u64 > MAX_REPLY || reply.last() != Some(&b'\n') {
        anyhow::bail!("invalid or oversized browser response");
    }
    let value: Value = serde_json::from_slice(&reply)?;
    if value["version"] != 1
        || value["id"] != request["id"]
        || !matches!(
            value["outcome"].as_str(),
            Some(
                "ok" | "truncated"
                    | "unavailable"
                    | "permission_denied"
                    | "stale"
                    | "uncertain"
                    | "invalid_request"
            )
        )
    {
        anyhow::bail!("mismatched browser response");
    }
    Ok(value)
}

#[cfg(not(unix))]
fn exchange(_: &Path, _: &Value, _: u64) -> anyhow::Result<Value> {
    anyhow::bail!("browser bridge requires a Unix host");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_contract_contains_no_action_or_grant_surface() {
        let value = request(&BrowserAction::Read {
            tab_id: 7,
            document_id: "synthetic-document".into(),
            max_chars: 32,
        });
        assert_eq!(value["op"], "read");
        assert_eq!(value["max_chars"], 32);
        assert!(value.get("grant").is_none());
    }

    #[cfg(unix)]
    fn serve(reply_id_matches: bool) -> (tempfile::TempDir, std::thread::JoinHandle<()>) {
        use std::os::unix::net::UnixListener;
        let root = tempfile::tempdir().expect("tempdir");
        let listener = UnixListener::bind(root.path().join("bridge.sock")).expect("listener");
        let thread = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream);
            let mut role = String::new();
            reader.read_line(&mut role).expect("role");
            assert_eq!(
                serde_json::from_str::<Value>(&role).unwrap()["role"],
                "client"
            );
            let mut payload = String::new();
            reader.read_line(&mut payload).expect("request");
            let req: Value = serde_json::from_str(&payload).unwrap();
            let id = if reply_id_matches {
                req["id"].clone()
            } else {
                json!("wrong")
            };
            writeln!(
                reader.get_mut(),
                "{}",
                json!({"version": 1, "id": id, "outcome": "permission_denied"})
            )
            .unwrap();
        });
        (root, thread)
    }

    #[test]
    #[cfg(unix)]
    fn preserves_denial_and_rejects_mismatched_responses() {
        for matches in [true, false] {
            let (root, worker) = serve(matches);
            let req = request(&BrowserAction::Tabs);
            let reply = exchange(&root.path().join("bridge.sock"), &req, 1000);
            if matches {
                assert_eq!(reply.unwrap()["outcome"], "permission_denied");
            } else {
                assert!(reply.is_err());
            }
            worker.join().unwrap();
        }
    }
}
