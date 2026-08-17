# ctx-mcp

A target-neutral, **read-only** MCP Resource server for deliberate access to
ctx Current Context and durable Bundles. MCP-capable agent hosts can discover
and read context without importing ctx libraries and without any automatic
prompt injection.

## Privacy model

- **Pull-only.** Listing and reading resources never injects model context,
  sends messages, invokes tools, or calls an AI provider. The MCP host decides
  whether a selected resource enters a prompt; ctx-mcp only supplies
  resources, annotations, provenance, and structured unavailable outcomes.
- **Resource availability is not prompt inclusion.** A resource appearing in
  `resources/list` means it *can* be read on request, nothing more.
- **Capture policy is honored.** Clipboard, selection, screenshot, page-text,
  and accessibility data that was disabled or denied at capture time simply
  does not exist in the state ctx-mcp serves; there is no live-capture surface
  through MCP and no stale-content substitution.
- **Stdio only.** The only transport is line-delimited JSON-RPC 2.0 over
  stdin/stdout. This crate contains **no network listener**; desktop context
  is never exposed on a socket. If a Streamable HTTP mode is ever added, it
  will be separately configured, authenticated, bounded, and disabled by
  default.
- **No target concepts.** ctx-mcp knows nothing about any agent host,
  account, session, or destination. Handoff/push flows are a separate,
  deliberate CLI surface (`ctx bundle send`), not part of this server.

## Host configuration

Configure your MCP host to spawn the binary over stdio, for example:

```json
{
  "mcpServers": {
    "ctx": {
      "command": "ctx-mcp"
    }
  }
}
```

Paths (state file, bundle directory) come from the regular ctx configuration
(`~/.config/ctx/config.toml`, `CTX__*` environment variables).

Environment overrides:

| Variable | Default | Meaning |
| --- | --- | --- |
| `CTX_MCP_DEVICE_ID` | sanitized hostname, else `local` | Device id used in the current-context URI |
| `CTX_MCP_MAX_ITEM_BYTES` | `4194304` | Max bytes served inline for one item read |
| `CTX_MCP_PAGE_SIZE` | `50` | `resources/list` page size |

## Resource URIs (v1)

| URI | Content |
| --- | --- |
| `ctx://devices/<device-id>/current` | Complete lightweight Current Context snapshot (replace-whole state with `sequence` and `updated_at`) |
| `ctx://bundles/<bundle-id>` | Bundle summary: full manifest (consent/redaction/provenance metadata preserved) plus per-item resource URIs |
| `ctx://bundles/<bundle-id>/items/<item-id>` | One Bundle item: text as `text`, images/files as typed base64 `blob` with MIME type and size |

Parameterized templates for the bundle URIs are advertised via
`resources/templates/list`. URI segments are validated; traversal attempts
and manifest paths that escape the bundle root are rejected with structured
errors. Local filesystem paths are implementation details and never become
resource identity. File items that merely *reference* a path outside the
bundle root are not served over MCP.

## Snapshots, not streams

`ctx://devices/<device-id>/current` always returns the complete current
snapshot. Its `sequence` field increments on every accepted context report.
Subscriptions are currently **deferred**: capabilities advertise
`subscribe: false` and `resources/subscribe` returns a structured error.
Consumers should re-read the snapshot rather than reconstructing state from
events; any future change notification will only ever be an invalidation
hint.

## Errors

| Code | Meaning |
| --- | --- |
| `-32002` | Resource not found (unknown device, bundle, or item) |
| `-32003` | Resource unavailable (unsafe manifest path, unreadable file, over the inline size limit — see `data.reason`) |
| `-32602` | Invalid params (malformed URI or cursor) |

Oversized binary items return `-32003` with
`data: {reason: "too_large", sizeBytes, limitBytes, mimeType}` instead of
forcing bytes into a model response; hosts can raise the bound or use the
local `.ctx` export.
