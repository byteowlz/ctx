//! ctx-mcp stdio entry point: line-delimited JSON-RPC over stdin/stdout.
//!
//! Stdio is the only transport. There is deliberately no network listener;
//! see README.md for the privacy model.

use std::io::{BufRead, Write};

use ctx_mcp::{Server, ServerConfig};

fn main() -> anyhow::Result<()> {
    let cfg = ctx_core::config::load(ctx_core::config::default_app_name())?;
    let server = Server::new(ServerConfig::from_paths(
        cfg.output.state_file.clone(),
        cfg.output.bundle_dir.clone(),
    ));

    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = server.handle_line(&line) {
            stdout.write_all(response.as_bytes())?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
        }
    }
    Ok(())
}
