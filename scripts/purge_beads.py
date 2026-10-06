# /// script
# requires-python = ">=3.11"
# ///
"""Archive clone-local legacy Beads integration; never invoke bd."""

from datetime import datetime, timezone
import os
from pathlib import Path
import re
import shutil
import subprocess
import uuid


CONFIG_KEYS = r"^(beads|bd)\.|^(merge|filter)\.(beads|bd)\."
OWNED_HEADER = re.compile(r"^#.*(?:\b(?:beads|bd)\b.*hook|hook.*\b(?:beads|bd)\b)", re.IGNORECASE | re.MULTILINE)
BEADS_REFERENCE = re.compile(r"\b(?:beads|bd)\b", re.IGNORECASE)
SAFE_LINE = re.compile(
    r"(?:if\s+(?:!\s+)?command -v bd\s*>/dev/null\s+2>&1;\s*then"
    r"|(?:exec\s+)?bd\s+[^;&|`]+"
    r"|(?:echo|printf)\s+(?:\"[^\"\n]*\"|'[^'\n]*')(?:\s+>&2)?"
    r"|exit\s+\d+|fi|else|then)"
)


def git(*args, optional=False):
    result = subprocess.run(["git", *args], capture_output=True, text=True)
    if result.returncode and not (optional and result.returncode == 1):
        raise SystemExit("Cannot inspect/update repository-local Git configuration")
    return result.stdout.strip()


def standalone_hook(content):
    if not OWNED_HEADER.search(content):
        return False
    for line in content.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if "$(" in line or "`" in line or not SAFE_LINE.fullmatch(line):
            return False
    return True


def owned_hooks(hooks):
    owned = []
    for hook in sorted(hooks.iterdir()) if hooks.exists() else []:
        if hook.name.endswith(".sample") or not hook.is_file():
            continue
        content = hook.read_text(errors="replace")
        if not BEADS_REFERENCE.search(content):
            continue
        if not standalone_hook(content):
            raise SystemExit(f"Mixed/unrecognized hook {hook.name}: manual review required; no changes made")
        owned.append(hook)
    return owned


def main():
    repo = Path(git("rev-parse", "--show-toplevel")).resolve()
    os.chdir(repo)
    common = Path(git("rev-parse", "--git-common-dir")).resolve()
    hook_config = git("config", "--show-scope", "--get", "core.hooksPath", optional=True)
    scope, _, configured = hook_config.partition("\t")
    if configured and scope not in {"local", "worktree"}:
        raise SystemExit("Shared/global hooks setting: manual review required; no changes made")
    hooks = Path(configured).resolve() if configured else common / "hooks"
    if not (hooks.is_relative_to(repo) or hooks.is_relative_to(common)):
        raise SystemExit("Shared/external hooks path: manual review required; no changes made")
    owned = owned_hooks(hooks)
    keys = git("config", "--local", "--name-only", "--get-regexp", CONFIG_KEYS, optional=True).splitlines()
    legacy = repo / ".beads"
    has_data = legacy.exists() or legacy.is_symlink()
    if not owned and not keys and not has_data:
        print("No legacy Beads integration found; .trx and unrelated hooks unchanged")
        return

    state = Path(os.environ.get("XDG_STATE_HOME", str(Path.home() / ".local/state"))).resolve()
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    backup = state / "ctx/beads-backups" / f"{stamp}-{uuid.uuid4().hex[:8]}"
    if backup.is_relative_to(repo):
        raise SystemExit("Backup location must be outside the repository; no changes made")
    backup.mkdir(parents=True, mode=0o700)
    if keys:
        shutil.copy2(common / "config", backup / "git-config")
    for hook in owned:
        shutil.move(str(hook), str(backup / hook.name))
    if has_data:
        shutil.move(str(legacy), str(backup / ".beads"))
    for key in keys:
        git("config", "--local", "--unset-all", key)
    print(f"Archived {len(owned)} Beads hooks, legacy data={has_data}; removed {len(keys)} local settings")
    print(f"Private backup: {backup}")
    print(".trx, unrelated hooks and global configuration unchanged")


if __name__ == "__main__":
    main()
