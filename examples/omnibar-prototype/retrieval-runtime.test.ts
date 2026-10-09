import { expect, test } from "bun:test";
import { isAbsolute, resolve } from "node:path";
import { EvokeWorker } from "./retrieval-worker";
import { candidates } from "./decision";

const real = process.env.CTX_TEST_RETRIEVAL === "true" && process.platform === "darwin" && process.arch === "arm64";
const cache = process.env.XDG_CACHE_HOME && isAbsolute(process.env.XDG_CACHE_HOME) ? process.env.XDG_CACHE_HOME : resolve(process.env.HOME!, ".cache");
const config = { enabled: true, backend: "cpu" as const, worker_path: resolve(cache, "ctx/retrieval-runtime/worker"), model_dir: resolve(cache, "ctx/retrieval-probe/model") };

test("uninstalled worker fails explicitly without spawning a shell or downloading", async () => {
  const worker = new EvokeWorker({ ...config, worker_path: "/ctx-synthetic-nonexistent/worker" });
  try { await expect(worker.search("light", [])).rejects.toThrow("unavailable"); }
  finally { worker.close(); }
});
test("disabled worker performs no inference", async () => {
  const worker = new EvokeWorker({ ...config, enabled: false });
  try { await expect(worker.search("light", [])).rejects.toThrow("disabled"); }
  finally { worker.close(); }
});
(real ? test : test.skip)("installed CPU runtime: real light-mode retrieval, scoped reindex, persistence and token budget", async () => {
  const worker = new EvokeWorker(config);
  try {
    const offered = candidates("macos", "desktop");
    const cold = await worker.search("switch to light mode", offered);
    expect(cold.source).toBe("Local Evoke"); expect(cold.ids).toContain("appearance.light");
    expect(cold.latencyMs).toBeGreaterThan(0); expect(cold.truncated).toBe(false);
    const warm = await worker.search("screen brighter", offered);
    expect(warm.ids).toContain("display.brightness");
    expect(warm.latencyMs).toBeLessThan(cold.latencyMs);
    const sound = candidates("macos", "desktop", "branches", "fixture-sound");
    const scoped = await worker.search("volume louder", sound);
    expect(scoped.ids.every(id => sound.some(item => item.id === id))).toBe(true);
    const truncated = await worker.search("a ".repeat(80), offered);
    expect(truncated.truncated).toBe(true); expect(truncated.ids).toEqual([]);
    // An error/token-budget response never kills an otherwise healthy persistent worker.
    expect((await worker.search("light mode", offered)).ids).toContain("appearance.light");
    const first = worker.search("audio", offered);
    await expect(worker.search("display", offered)).rejects.toThrow("busy");
    await first;
    console.log(JSON.stringify({ synthetic: true, requestedBackend: "cpu", coldIndexAndQueryMs: cold.latencyMs, warmQueryMs: warm.latencyMs, correctScope: true }));
  } finally { worker.close(); }
}, 20_000);
