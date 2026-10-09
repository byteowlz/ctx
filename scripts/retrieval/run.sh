#!/usr/bin/env bash
set -euo pipefail
# Headless synthetic probe: no UI, Accessibility permission, execution or private data.
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/ctx/retrieval-probe"
OUT="${1:-$CACHE/results}"
SOURCE=fda0e480329150d47e60735ad80ad90c41f01b09
MODEL=4ef29e090fd5a8480cc0830132d8011ac68dd71b
[[ $(uname -s) == Darwin ]] || { printf 'CoreML benchmark requires macOS\n' >&2; exit 1; }
mkdir -p "$CACHE" "$OUT"
if [[ ! -d "$CACHE/FluidUse/.git" ]]; then git clone --filter=blob:none https://github.com/FluidInference/FluidUse.git "$CACHE/FluidUse"; fi
if [[ $(git -C "$CACHE/FluidUse" rev-parse HEAD) != "$SOURCE" ]]; then
  [[ -z $(git -C "$CACHE/FluidUse" status --porcelain) ]] || { printf 'Cached source is modified; refusing checkout\n' >&2; exit 1; }
  git -C "$CACHE/FluidUse" fetch origin "$SOURCE"
  git -C "$CACHE/FluidUse" checkout --detach "$SOURCE"
fi
git -C "$CACHE/FluidUse" diff-index --quiet HEAD -- || { printf 'Pinned source is modified\n' >&2; exit 1; }
mkdir -p "$CACHE/model"
[[ ! -e "$CACHE/model/granite-embedding-30m-sparse-L64-fp16.mlmodelc" ]] || { printf 'Unverified precompiled model in cache; remove it before benchmarking\n' >&2; exit 1; }
while read -r sha file; do
  path="$CACHE/model/$file"
  if [[ ! -f "$path" ]] || [[ $(shasum -a 256 "$path" | awk '{print $1}') != "$sha" ]]; then
    mkdir -p "$(dirname "$path")"
    curl -fL --retry 2 "https://huggingface.co/FluidInference/granite-embedding-30m-sparse-coreml/resolve/$MODEL/$file" -o "$path.tmp"
    [[ $(shasum -a 256 "$path.tmp" | awk '{print $1}') == "$sha" ]] || { rm -f "$path.tmp"; printf 'Model checksum mismatch\n' >&2; exit 1; }
    mv "$path.tmp" "$path"
  fi
done <<'ASSETS'
c82d13490aa7919a83915f810cb16f614cb2737121ff98cee7f428ee8c851ea9 config.json
33465117406b9007673e8ba283f7f1383d9b5094df947481af60eec94ed7d7bd tokenizer.json
1910f036d332ab96ad0152fb89634a02de44a956192d8775389c893cb8224ff9 granite-embedding-30m-sparse-L64-fp16.mlpackage/Data/com.apple.CoreML/model.mlmodel
6dea4b191d4ddeb2804e4474048b36de09ce3ff8a999164ca470796dc27ebe78 granite-embedding-30m-sparse-L64-fp16.mlpackage/Data/com.apple.CoreML/weights/weight.bin
01754dbd180d03a7c6917991996fc2fb43d730a56c87eb0f3dd3cc1852ec958f granite-embedding-30m-sparse-L64-fp16.mlpackage/Manifest.json
ASSETS
UP="$CACHE/FluidUse/Sources/FluidUse"
bun "$ROOT/scripts/retrieval/fixtures.ts" "$OUT/fixtures.json"
swiftc -O -warnings-as-errors -parse-as-library "$UP/Qwen/QwenBPETokenizer.swift" "$UP/Evoke/EvokeTypes.swift" "$UP/Evoke/EvokeManager.swift" "$ROOT/scripts/retrieval/Probe.swift" -o "$CACHE/probe"
"$CACHE/probe" --self-test
export EVOKE_TOKEN_REFERENCE="$CACHE/FluidUse/Tests/FluidUseTests/Fixtures/evoke-reference.json"
for backend in ane gpu cpu; do
  if ! "$CACHE/probe" "$CACHE/model" "$OUT/fixtures.json" "$OUT/$backend.json" "$backend"; then
    printf 'Backend %s failed; retaining its failure JSON and continuing comparison\n' "$backend" >&2
  fi
done
bun "$ROOT/scripts/retrieval/report.ts" "$OUT"
