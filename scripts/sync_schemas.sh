#!/usr/bin/env bash
# Sync ctx JSON schemas to the shared schemas repo.
#
# Copies the config schema and the context bundle manifest schema from
# examples/ into ../schemas/ctx/, then commits and pushes.
set -euo pipefail

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST_DIR="$SRC_DIR/../schemas/ctx"

mkdir -p "$DEST_DIR"

# Pull first while the tree is clean (before we write any files).
cd "$DEST_DIR"
git pull --rebase
cd "$SRC_DIR"

SCHEMAS=(
  "config.schema.json:ctx.config.schema.json"
  "bundle.schema.json:ctx.bundle.schema.json"
)

changed=0
for pair in "${SCHEMAS[@]}"; do
  src="${pair%%:*}"
  dst_name="${pair##*:}"
  src_path="$SRC_DIR/examples/$src"
  dst_path="$DEST_DIR/$dst_name"
  if [ ! -f "$src_path" ]; then
    echo "Source schema not found: $src_path" >&2
    exit 1
  fi
  if [ ! -f "$dst_path" ] || ! cmp -s "$src_path" "$dst_path"; then
    cp "$src_path" "$dst_path"
    echo "Copied $src -> $dst_name"
    changed=1
  else
    echo "Unchanged: $dst_name"
  fi
done

if [ "$changed" -eq 0 ]; then
  echo "No schema changes to sync."
  exit 0
fi

cd "$DEST_DIR"
git add .
git commit -m "feat: updated ctx schemas (config, bundle manifest)"
git push
echo "Committed and pushed schema changes"
