import { describe, expect, test } from "bun:test";
import { candidates, payload, suggestions } from "./decision";
import { handler, type Settings } from "./server";
import { SelectionSession } from "./state.js";

const offered = candidates("macos", "desktop");
function raw(choice = "appearance.light", options = offered) {
  return { answers: { route: { type: "choice", choice, probabilities: Object.fromEntries([...options.map(item => item.id), "REFUSE"].map(id => [id, id === choice ? 1 : 0])) } } };
}
const settings: Settings = { origin: "http://localhost:4784", reviewKey: "review-test", endpoint: "http://upstream.invalid/v1/systemone", model: "jev-latest", listenerToken: "listener-test", virtualKey: "virtual-test" };
function request(body: unknown = { query: "light mode", platform: "macos", fixture: "desktop" }, headers: Record<string,string> = {}) {
  return new Request(`${settings.origin}/api/suggest`, { method: "POST", headers: { origin: settings.origin, "Content-Type": "application/json", "x-prototype-key": settings.reviewKey, ...headers }, body: JSON.stringify(body) });
}

describe("decision envelopes", () => {
  test("offers OS controls with platform/context filtering", () => {
    expect(offered.some(item => item.id === "appearance.light")).toBe(true);
    expect(offered.some(item => item.id === "theme.pick")).toBe(false);
    expect(candidates("omarchy", "desktop").some(item => item.id === "theme.pick")).toBe(true);
    expect(candidates("linux", "unavailable").some(item => item.id === "screenshot.capture")).toBe(false);
    expect(candidates("linux", "selection").some(item => item.id === "selection.speak")).toBe(true);
    expect(payload("light mode", "macos", "desktop", "jev/jev-latest").state.context_source).toContain("synthetic");
  });
  test("branch candidates are shallow, lazy and context/platform filtered", () => {
    const root = candidates("macos", "desktop", "branches");
    expect(root.some(item => item.id === "fixture.appearance" && item.node === "fixture-appearance")).toBe(true);
    expect(root.some(item => item.id === "appearance.light")).toBe(false);
    const child = candidates("macos", "desktop", "branches", "fixture-appearance");
    expect(child.map(item => item.id)).toEqual(["appearance.light", "appearance.dark", "appearance.open"]);
    expect(candidates("omarchy", "desktop", "branches", "fixture-appearance").some(item => item.id === "theme.pick")).toBe(true);
    expect(() => candidates("macos", "desktop", "flat", "fixture-appearance")).toThrow();
    expect(() => candidates("macos", "desktop", "branches", "appearance.light")).toThrow();
  });
  test("normalizes choices; refuses rather than inventing a match", () => {
    expect(suggestions(raw(), offered)[0]?.id).toBe("appearance.light");
    expect(suggestions(raw("REFUSE"), offered)).toEqual([]);
  });
  test("rejects malformed, omitted, out-of-range and unlisted probabilities", () => {
    for (const value of [null, { answers: { route: { choice: "appearance.light", probabilities: {} } } }, raw("not-listed")]) expect(() => suggestions(value, offered)).toThrow();
    const bad = raw(); bad.answers.route.probabilities["appearance.light"] = NaN;
    expect(() => suggestions(bad, offered)).toThrow();
  });
});

describe("selection lifecycle", () => {
  test("stale responses cannot arm countdown", () => {
    const state = new SelectionSession(); const old = state.invalidate(); state.invalidate();
    expect(state.accept(old, offered, 5000, 0)).toBe(false); expect(state.deadline).toBeNull();
  });
  test("optional timeout selects first exactly once", () => {
    const state = new SelectionSession(); state.accept(state.generation, offered, 5000, 0);
    expect(state.expire(4999)).toBeNull(); expect(state.expire(5000)?.item.id).toBe(offered[0]?.id);
    expect(state.expire(9000)).toBeNull(); expect(state.choose(1)).toBeNull();
  });
  test("manual navigation, edits and cancel suppress auto-selection", () => {
    const state = new SelectionSession(); state.accept(0, offered, 5000, 0); state.move(1);
    expect(state.expire(9000)).toBeNull(); expect(state.choose()?.reason).toBe("manual");
    state.invalidate(); expect(state.choose()).toBeNull();
    state.accept(state.generation, offered, 0, 0); expect(state.deadline).toBeNull();
  });
});

describe("runtime boundary", () => {
  test("uses real EAVS shape and hides upstream credentials", async () => {
    let body: unknown;
    const serve = handler(settings, (async (_url, options) => {
      body = JSON.parse(options?.body as string);
      expect((options?.headers as Record<string,string>)["X-Eavs-Token"]).toBe("listener-test");
      return Response.json(raw());
    }) as typeof fetch);
    const response = await serve(request()); expect(response.status).toBe(200);
    expect((await response.json()).items[0].id).toBe("appearance.light");
    expect((body as {model:string}).model).toBe("jev-latest");
    const info = await serve(new Request(`${settings.origin}/api/catalog`));
    const text = await info.text(); expect(text).not.toContain("listener-test"); expect(text).not.toContain("virtual-test");
  });
  test("rejects cross-origin, wrong key and malformed requests without inference", async () => {
    const serve = handler(settings, (() => { throw new Error("must not fetch"); }) as typeof fetch);
    expect((await serve(request({}, { origin: "https://evil.invalid" }))).status).toBe(403);
    expect((await serve(request({}, { "x-prototype-key": "wrong" }))).status).toBe(403);
    for (const body of [{query:"x".repeat(513), platform:"macos",fixture:"desktop"}, {query:"x",platform:"other",fixture:"desktop"}, {}]) {
      expect((await serve(request(body))).status).toBe(400);
    }
  });
  test("unavailable or invalid upstream never falls back to fixture routing", async () => {
    const missing = handler({...settings, virtualKey:""}); expect((await missing(request())).status).toBe(503);
    const broken = handler(settings, (async () => Response.json({secret:"private-value"}, {status:401})) as typeof fetch);
    const response = await broken(request()); expect(response.status).toBe(502); expect(await response.text()).not.toContain("private-value");
    const invalid = handler(settings, (async () => Response.json({choices:[]})) as typeof fetch);
    expect((await invalid(request())).status).toBe(502);
  });
  test("root decision does not precompute children; selection makes one fresh hop", async () => {
    const calls: any[] = [];
    const serve = handler(settings, (async (_url, options) => {
      const body = JSON.parse(options?.body as string); calls.push(body);
      const child = body.state.node !== "root";
      return Response.json(raw(child ? "appearance.light" : "fixture.appearance", candidates("macos", "desktop", "branches", body.state.node)));
    }) as typeof fetch);
    const root = await serve(request({query:"light mode", platform:"macos", fixture:"desktop", presentation:"branches", node:"root"}));
    const branch = (await root.json()).items[0];
    expect(branch.kind).toBe("branch"); expect(calls.length).toBe(1);
    expect(Object.keys(calls[0].questions)).toEqual(["route"]);
    expect(calls[0].questions.route.criteria["appearance.light"]).toBeUndefined();
    const child = await serve(request({query:"light mode", platform:"macos", fixture:"desktop", presentation:"branches", node:branch.node}));
    expect((await child.json()).items[0].id).toBe("appearance.light"); expect(calls.length).toBe(2);
    expect(calls[1].questions.route.criteria["sound.volume"]).toBeUndefined();
    const wrongScope = handler(settings, (async () => Response.json(raw())) as typeof fetch);
    expect((await wrongScope(request({query:"light mode", platform:"macos", fixture:"desktop", presentation:"branches", node:"root"}))).status).toBe(502);
  });
  test("unknown/deeper branches fail before inference and catalog declares branch metadata", async () => {
    const serve = handler(settings, (() => { throw new Error("must not infer"); }) as typeof fetch);
    for (const [presentation,node] of [["branches","invented"],["flat","fixture-appearance"],["branches","appearance.light"]]) {
      expect((await serve(request({query:"light mode",platform:"macos",fixture:"desktop",presentation,node}))).status).toBe(400);
    }
    const catalog = await (await serve(new Request(`${settings.origin}/api/catalog`))).json();
    expect(catalog.branches[0].children).toContain("appearance.light");
  });
  test("oversized/malformed requests and responses remain bounded", async () => {
    const serve = handler(settings, (async () => new Response("x".repeat(128_001))) as typeof fetch);
    expect((await serve(request(null))).status).toBe(400);
    expect((await serve(request({query:"x".repeat(4100)}))).status).toBe(400);
    expect((await serve(request())).status).toBe(502);
  });
  test("only two authorized in-flight requests are permitted", async () => {
    let release!: () => void; const gate = new Promise<void>(resolve => release = resolve);
    const serve = handler(settings, (async () => { await gate; return Response.json(raw()); }) as typeof fetch);
    const first = serve(request()), second = serve(request());
    expect((await serve(request())).status).toBe(429);
    release(); expect((await first).status).toBe(200); expect((await second).status).toBe(200);
  });
  test("no browser or arbitrary local files are served", async () => {
    const serve = handler(settings);
    expect((await serve(new Request(`${settings.origin}/README.md`))).status).toBe(404);
    expect((await serve(new Request(`${settings.origin}/config.toml`))).status).toBe(404);
    expect((await serve(new Request(`http://evil.invalid:4784/api/catalog`))).status).toBe(403);
  });
});
