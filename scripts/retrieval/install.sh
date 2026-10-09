#!/usr/bin/env bash
set -euo pipefail
# Explicit install only. This never launches a window or joins a tmux/herdr session.
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/ctx/retrieval-probe"
RUNTIME="${XDG_CACHE_HOME:-$HOME/.cache}/ctx/retrieval-runtime"
[[ $(uname -s) == Darwin && $(uname -m) == arm64 ]] || { printf 'This optional CoreML backend requires Apple Silicon macOS\n' >&2; exit 1; }
SOURCE=$(bun -e 'console.log((await Bun.file(process.argv[1]).json()).source)' "$ROOT/scripts/retrieval/assets.json")
MODEL=$(bun -e 'console.log((await Bun.file(process.argv[1]).json()).model)' "$ROOT/scripts/retrieval/assets.json")
umask 077
mkdir -p "$CACHE/model" "$RUNTIME"
if [[ ! -d "$CACHE/FluidUse/.git" ]]; then git clone --filter=blob:none https://github.com/FluidInference/FluidUse.git "$CACHE/FluidUse"; fi
if [[ $(git -C "$CACHE/FluidUse" rev-parse HEAD) != "$SOURCE" ]]; then
  [[ -z $(git -C "$CACHE/FluidUse" status --porcelain) ]] || { printf 'Modified cached source; refusing checkout\n' >&2; exit 1; }
  git -C "$CACHE/FluidUse" fetch origin "$SOURCE"
  git -C "$CACHE/FluidUse" checkout --detach "$SOURCE"
fi
git -C "$CACHE/FluidUse" diff-index --quiet HEAD -- || { printf 'Pinned source is modified\n' >&2; exit 1; }
[[ ! -e "$CACHE/model/granite-embedding-30m-sparse-L64-fp16.mlmodelc" ]] || { printf 'Unverified compiled model override; refusing install\n' >&2; exit 1; }
while read -r sha file; do
  path="$CACHE/model/$file"
  if [[ ! -f "$path" ]] || [[ $(shasum -a 256 "$path" | awk '{print $1}') != "$sha" ]]; then
    mkdir -p "$(dirname "$path")"
    curl -fL --retry 2 "https://huggingface.co/FluidInference/granite-embedding-30m-sparse-coreml/resolve/$MODEL/$file" -o "$path.tmp"
    [[ $(shasum -a 256 "$path.tmp" | awk '{print $1}') == "$sha" ]] || { rm -f "$path.tmp"; printf 'Model checksum mismatch\n' >&2; exit 1; }
    mv "$path.tmp" "$path"
  fi
done < <(bun -e 'for (const [file, sha] of Object.entries((await Bun.file(process.argv[1]).json()).files)) console.log(`${sha} ${file}`)' "$ROOT/scripts/retrieval/assets.json")
UP="$CACHE/FluidUse/Sources/FluidUse"
swiftc -O -warnings-as-errors -parse-as-library "$UP/Qwen/QwenBPETokenizer.swift" "$UP/Evoke/EvokeTypes.swift" "$UP/Evoke/EvokeManager.swift" "$ROOT/scripts/retrieval/Worker.swift" -o "$RUNTIME/worker.tmp"
cp "$ROOT/scripts/retrieval/assets.json" "$RUNTIME/assets.json"
cp "$CACHE/FluidUse/LICENSE" "$RUNTIME/LICENSE-FluidUse"
mv "$RUNTIME/worker.tmp" "$RUNTIME/worker"
printf 'Installed pinned optional worker. Enable [retrieval] enabled = true in ctx/config.toml; restart adapter. No settings changed.\n'
