# AGENTS.md

## Build/Test Commands

```bash
cargo check                      # Type-check all crates
cargo build                      # Build all crates
cargo test                       # Run all tests
cargo test -p ctx-core           # Test single crate
cargo test test_name             # Run single test by name
cargo clippy                     # Lint (fix all warnings)
cargo fmt --check                # Check formatting
```

## Code Style

- **Rust 2024 edition** with workspace structure (ctx-api, ctx-cli, ctx-core, ctx-mcp)
- Run `cargo check` after every change; fix all errors before committing
- Imports: std first, then external crates, then local modules (alphabetized)
- Use `thiserror` for library errors, `anyhow` for application errors
- Config: `config` crate with XDG paths (`$XDG_CONFIG_HOME` with `~/.config` fallback/sytem defaults)
- Config: env-variables have priority over config
- Data: `$XDG_DATA_HOME` (fallback `~/.local/share`/system defaults), State: `$XDG_STATE_HOME` (fallback `~/.local/state`/system defaults)
- No emojis in code, comments, or commit messages
- Prefer `Result<T, E>` over panics; degrade gracefully when permissions/APIs unavailable

---

## Issue Tracking with trx

**IMPORTANT**: This project uses **trx** for ALL issue tracking.

### Why trx?

- Dependency-aware: Track blockers and relationships between issues
- Git-friendly: Auto-syncs to JSONL for version control
- Agent-optimized: JSON output, ready work detection, discovered-from links
- Prevents duplicate tracking systems and confusion

### Quick Start

**Check for ready work:**

```bash
trx ready --json
```

**Create new issues:**

```bash
trx create "Issue title" -t bug|feature|task -p 0-4 --json
trx create "Issue title" -p 1 --deps discovered-from:trx-123 --json
```

**Claim and update:**

```bash
trx update trx-42 --status in_progress --json
trx update trx-42 --priority 1 --json
```

**Complete work:**

```bash
trx close trx-42 --reason "Completed" --json
```

### Issue Types

- `bug` - Something broken
- `feature` - New functionality
- `task` - Work item (tests, docs, refactoring)
- `epic` - Large feature with subtasks
- `chore` - Maintenance (dependencies, tooling)

### Priorities

- `0` - Critical (security, data loss, broken builds)
- `1` - High (major features, important bugs)
- `2` - Medium (default, nice-to-have)
- `3` - Low (polish, optimization)
- `4` - Backlog (future ideas)

### Workflow for AI Agents

1. **Check ready work**: `trx ready` shows unblocked issues
2. **Claim your task**: `trx update <id> --status in_progress`
3. **Work on it**: Implement, test, document
4. **Discover new work?** Create linked issue:
   - `trx create "Found bug" -p 1 --deps discovered-from:<parent-id>`
5. **Complete**: `trx close <id> --reason "Done"`
6. **Commit together**: Always commit the `.beads/issues.jsonl` file together with the code changes so issue state stays in sync with code state

### Auto-Sync

trx automatically syncs with git

### Managing AI-Generated Planning Documents

AI assistants often create planning and design documents during development:

- PLAN.md, IMPLEMENTATION.md, ARCHITECTURE.md
- DESIGN.md, CODEBASE_SUMMARY.md, INTEGRATION_PLAN.md
- TESTING_GUIDE.md, TECHNICAL_DESIGN.md, and similar files

**Best Practice: Use a dedicated directory for these ephemeral files**

**Recommended approach:**

- Create a `history/` directory in the project root
- Store ALL AI-generated planning/design docs in `history/`
- Keep the repository root clean and focused on permanent project files
- Only access `history/` when explicitly asked to review past planning

**Example .gitignore entry (optional):**

```
# AI planning documents (ephemeral)
history/
```

**Benefits:**

- Clean repository root
- Clear separation between ephemeral and permanent documentation
- Easy to exclude from version control if desired
- Preserves planning history for archeological research
- Reduces noise when browsing the project

### Important Rules

- Use trx for ALL task tracking
- Always use `--json` flag for programmatic use
- Link discovered work with `discovered-from` dependencies
- Check `trx ready` before asking "what should I work on?"
- Store AI planning docs in `history/` directory
- Do NOT create markdown TODO lists
- Do NOT use external issue trackers
- Do NOT duplicate tracking systems
- Do NOT clutter repo root with planning documents

For more details, see README.md and QUICKSTART.md.
