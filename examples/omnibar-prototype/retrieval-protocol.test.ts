import { afterEach, expect, test } from "bun:test";
import { chmod, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import assets from "../../scripts/retrieval/assets.json";
import { EvokeWorker } from "./retrieval-worker";
import { candidates } from "./decision";

const paths: string[] = [];
afterEach(async () => { for (const path of paths.splice(0)) await rm(path, { recursive: true, force: true }); });
const apple = process.platform === "darwin" && process.arch === "arm64";
async function fake(output: string, manifest: any = assets) {
  const path = await mkdtemp(resolve(tmpdir(), "ctx-retrieval-protocol-")); paths.push(path);
  // Fixed synthetic output only. Never interpolate input/user text into commands.
  await Bun.write(resolve(path, "worker"), `#!/usr/bin/env bash\nprintf '%s\\n' '{"ready":true}'\nIFS= read -r ignored\nprintf '%s\\n' '${output}'\nsleep 2\n`);
  await chmod(resolve(path, "worker"), 0o700);
  await Bun.write(resolve(path, "assets.json"), JSON.stringify(manifest));
  return new EvokeWorker({ enabled: true, backend: "cpu", worker_path: resolve(path, "worker"), model_dir: resolve(path, "model") });
}
(apple ? test : test.skip)("persistent transport rejects wrong tickets, unoffered/duplicate IDs, nonfinite and oversized output", async () => {
  const inputs = [
    '{"id":999,"ids":[],"latencyMs":3,"truncated":false}',
    '{"id":1,"ids":["unknown"],"latencyMs":3,"truncated":false}',
    '{"id":1,"ids":["appearance.light","appearance.light"],"latencyMs":3,"truncated":false}',
    '{"id":1,"ids":[],"latencyMs":-1,"truncated":false}',
    '{"id":1,"ids":[],"latencyMs":1e999,"truncated":false}',
    "x".repeat(20_000),
  ];
  for (const input of inputs) {
    const worker = await fake(input);
    try { await expect(worker.search("light", candidates("macos", "desktop"))).rejects.toThrow("unavailable"); }
    finally { worker.close(); }
  }
}, 10_000);
(apple ? test : test.skip)("unpinned installed manifest is rejected before helper startup", async () => {
  const worker = await fake('{"id":1,"ids":[],"latencyMs":3,"truncated":false}', { ...assets, model: "not-the-pinned-model" });
  try { await expect(worker.search("light", [])).rejects.toThrow("unavailable"); }
  finally { worker.close(); }
});
