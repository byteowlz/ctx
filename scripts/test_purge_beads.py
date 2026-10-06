# /// script
# requires-python = ">=3.11"
# ///
"""Replay the actual Git hook warning in isolated repositories."""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("purge_beads.py").resolve()
HOOK = '''#!/usr/bin/env bash
# bd (beads) post-merge hook
echo "Warning: bd command not found, skipping post-merge import" >&2
exit 0
'''


class PurgeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.git("init", "-q")
        self.hooks = self.repo / ".git/hooks"
        self.git("config", "core.hooksPath", str(self.hooks))
        self.env = {**os.environ, "XDG_STATE_HOME": str(self.root / "state")}

    def git(self, *args):
        return subprocess.run(["git", "-C", str(self.repo), *args], check=True,
                              capture_output=True, text=True)

    def hook(self, name, content):
        path = self.hooks / name
        path.write_text(content)
        path.chmod(0o755)
        return path

    def purge(self):
        return subprocess.run([sys.executable, str(SCRIPT)], cwd=self.repo,
                              env=self.env, capture_output=True, text=True)

    def test_warning_disappears_and_repeat_is_noop(self):
        hook = self.hook("post-merge", HOOK)
        before = self.git("hook", "run", "--ignore-missing", "post-merge")
        self.assertIn("bd command not found", before.stderr)
        self.assertEqual(self.purge().returncode, 0)
        self.assertFalse(hook.exists())
        after = self.git("hook", "run", "--ignore-missing", "post-merge")
        self.assertNotIn("bd command not found", after.stderr)
        archived = list((self.root / "state").rglob("post-merge"))
        self.assertEqual(len(archived), 1)
        self.assertEqual(archived[0].read_text(), HOOK)
        self.assertEqual(self.purge().returncode, 0)
        self.assertEqual(len(list((self.root / "state").rglob("post-merge"))), 1)

    def test_unrelated_hooks_and_trx_are_untouched(self):
        unrelated = self.hook("pre-commit", "#!/usr/bin/env bash\ncargo fmt --check\n")
        self.hook("post-merge", HOOK)
        tracker = self.repo / ".trx/issues.jsonl"
        tracker.parent.mkdir()
        tracker.write_text('{"id":"synthetic-test"}\n')
        self.assertEqual(self.purge().returncode, 0)
        self.assertIn("cargo fmt", unrelated.read_text())
        self.assertEqual(tracker.read_text(), '{"id":"synthetic-test"}\n')

    def test_mixed_hook_is_not_removed(self):
        mixed = self.hook("post-merge", HOOK + "cargo check\n")
        self.assertNotEqual(self.purge().returncode, 0)
        self.assertEqual(mixed.read_text(), HOOK + "cargo check\n")

    def test_data_and_local_config_are_archived(self):
        legacy = self.repo / ".beads"
        legacy.mkdir()
        (legacy / "issues.jsonl").write_text('{"id":"synthetic-legacy"}\n')
        self.git("config", "beads.syncBranch", "legacy")
        self.git("config", "merge.beads.driver", "bd merge")
        self.git("config", "user.syntheticSetting", "keep")
        self.assertEqual(self.purge().returncode, 0)
        self.assertFalse(legacy.exists())
        archived = list((self.root / "state").rglob(".beads/issues.jsonl"))
        self.assertEqual(len(archived), 1)
        self.assertEqual(archived[0].read_text(), '{"id":"synthetic-legacy"}\n')
        self.assertEqual(self.git("config", "--get", "user.syntheticSetting").stdout.strip(), "keep")
        keys = self.git("config", "--local", "--name-only", "--list").stdout
        self.assertNotIn("beads.syncbranch", keys.lower())
        self.assertNotIn("merge.beads.driver", keys.lower())

    def test_external_shared_hooks_are_not_modified(self):
        external = self.root / "shared-hooks"
        external.mkdir()
        hook = external / "post-merge"
        hook.write_text(HOOK)
        self.git("config", "core.hooksPath", str(external))
        self.assertNotEqual(self.purge().returncode, 0)
        self.assertEqual(hook.read_text(), HOOK)

    def test_global_hook_setting_is_preserved_even_inside_repository(self):
        hook = self.hook("post-merge", HOOK)
        self.git("config", "--unset", "core.hooksPath")
        global_config = self.root / "global-config"
        global_config.write_text(f'[core]\n\thooksPath = {self.hooks}\n')
        self.env["GIT_CONFIG_GLOBAL"] = str(global_config)
        self.assertNotEqual(self.purge().returncode, 0)
        self.assertEqual(hook.read_text(), HOOK)

    def test_standard_legacy_import_wrapper(self):
        hook = self.hook("post-merge", '''#!/usr/bin/env bash
# Post-merge hook for Beads
if command -v bd >/dev/null 2>&1; then
    bd import --no-daemon -i .beads/issues.jsonl
else
    echo "Warning: bd command not found, skipping post-merge import" >&2
fi
''')
        self.assertEqual(self.purge().returncode, 0)
        self.assertFalse(hook.exists())

    def test_custom_repository_hook_path(self):
        self.hooks = self.repo / "custom-hooks"
        self.hooks.mkdir()
        self.git("config", "core.hooksPath", "custom-hooks")
        hook = self.hook("post-merge", HOOK)
        self.assertEqual(self.purge().returncode, 0)
        self.assertFalse(hook.exists())


if __name__ == "__main__":
    unittest.main()
