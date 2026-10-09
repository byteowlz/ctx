import { afterEach, expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { candidates, payload } from "./decision";
import { loadRetrievalConfig } from "./retrieval-config";
import { handler, type Settings } from "./server";
import type { Retriever } from "./retrieval-worker";

const temporary: string[] = [];
afterEach(async () => { for (const path of temporary.splice(0)) await rm(path, { recursive: true, force: true }); });
const settings: Settings = { origin: "http://127.0.0.1:4784", reviewKey: "synthetic-local-capability", endpoint: "http://127.0.0.1:3033/jev/v1/systemone", model: "jev-latest", virtualKey: "synthetic-key", listenerToken: "synthetic-token" };
function refuse() {
  return Response.json({ answers: { route: { choice: "REFUSE", probabilities: { ...Object.fromEntries(candidates("macos", "desktop").map(item => [item.id, 0])), REFUSE: 1 } } } });
}
function request(path = "/api/retrieve", body: any = { query: "light", platform: "macos", fixture: "desktop" }, key = settings.reviewKey) {
  return new Request(settings.origin + path, { method: "POST", headers: { Origin: settings.origin, "X-Prototype-Key": key, "Content-Type": "application/json" }, body: JSON.stringify(body) });
}
test("shared retrieval config is opt-in, respects precedence and rejects invalid backends/settings", async () => {
  const home = await mkdtemp(resolve(tmpdir(), "ctx-retrieval-")); temporary.push(home);
  const dir = resolve(home, ".config/ctx"), local = resolve(home, "working");
  const env = { HOME: home };
  expect((await loadRetrievalConfig(env, local)).enabled).toBe(false);
  await Bun.write(resolve(dir, "config.toml"), "[eavs]\nmodel='jev-latest'\n[retrieval]\nenabled=true\nbackend='ane'\n");
  await Bun.write(resolve(local, "ctx.toml"), "[retrieval]\nbackend='cpu'\n");
  expect((await loadRetrievalConfig(env, local)).backend).toBe("cpu");
  expect((await loadRetrievalConfig({ ...env, CTX__RETRIEVAL__ENABLED: "false" }, local)).enabled).toBe(false);
  const explicit = resolve(home, "explicit.toml");
  await Bun.write(explicit, "[retrieval]\nenabled=true\nbackend='ane'\n");
  expect((await loadRetrievalConfig({ ...env, CTX_CONFIG: explicit, CTX__RETRIEVAL__ENABLED: "false" }, local)).enabled).toBe(true);
  for (const content of ["backend='gpu'", "enabled='true'", "unknown=true", "model_dir='relative'"]) {
    await Bun.write(explicit, `[retrieval]\n${content}\n`);
    await expect(loadRetrievalConfig({ ...env, CTX_CONFIG: explicit }, local)).rejects.toThrow();
  }
  await expect(loadRetrievalConfig({ ...env, CTX_CONFIG: resolve(home, "missing") }, local)).rejects.toThrow();
  expect(await Bun.file(resolve(dir, "config.toml")).text()).toContain("[eavs]");
});
test("local transport is private, bounded, current-scope only, and needs no provider credentials", async () => {
  let calls = 0;
  const retriever: Retriever = {
    enabled: true, close() {}, async search(_query, offered) {
      calls++; expect(offered).toEqual(candidates("macos", "desktop", "branches", "fixture-appearance"));
      return { ids: offered.slice(0, 2).map(item => item.id), source: "Local Evoke", truncated: false, latencyMs: 3 };
    },
  };
  const handle = handler({ ...settings, virtualKey: "", listenerToken: "" }, (() => { throw new Error("Remote must not be used"); }) as typeof fetch, retriever);
  const input = { query: "light", platform: "macos", fixture: "desktop", presentation: "branches", node: "appearance" };
  // Discover branch nodes from declared data, rather than guessing catalog aliases.
  input.node = (await import("./decision")).branches.find(item => item.id === "fixture.appearance")!.node!;
  const response = await handle(request("/api/retrieve", input));
  expect(response.status).toBe(200); expect((await response.json()).source).toBe("Local Evoke");
  expect((await handle(request("/api/retrieve", input, "wrong"))).status).toBe(403);
  for (const bad of [{ ...input, node: "unknown" }, { ...input, query: "x".repeat(513) }, { ...input, fixture: "private" }]) {
    expect((await handle(request("/api/retrieve", bad))).status).toBe(400);
  }
  expect(calls).toBe(1);
});
test("local failure does not replace Jev or shrink its offered catalog", async () => {
  let remoteBody: any;
  const upstream = (async (_url: any, init: any) => { remoteBody = JSON.parse(init.body); return refuse(); }) as typeof fetch;
  const retriever: Retriever = { enabled: true, close() {}, async search() { throw new Error("private helper diagnostic"); } };
  const handle = handler(settings, upstream, retriever);
  const failed = await handle(request()); expect(failed.status).toBe(503); expect(await failed.text()).not.toContain("private helper");
  const decision = await handle(request("/api/suggest")); expect(decision.status).toBe(200);
  expect(remoteBody).toEqual(payload("light", "macos", "desktop", settings.model));
  expect((await decision.json()).items).toEqual([]);
  expect((await handler(settings)(request())).status).toBe(503);
});
test("separate local concurrency cap does not consume the Jev rate budget", async () => {
  let release!: () => void;
  const wait = new Promise<void>(yes => { release = yes; });
  const retriever: Retriever = { enabled: true, close() {}, async search() { await wait; return { ids: [], source: "Local Evoke", latencyMs: 3, truncated: false }; } };
  const handle = handler(settings, (async () => refuse()) as typeof fetch, retriever);
  const first = handle(request()), second = handle(request());
  await Bun.sleep(10);
  expect((await handle(request())).status).toBe(429);
  expect((await handle(request("/api/suggest"))).status).toBe(200);
  release(); await Promise.all([first, second]);
});
