import { expect, test } from "bun:test";
import catalog from "./catalog.json";
import { candidates, payload, platforms, type ContextFixture } from "./decision";

const contexts: ContextFixture[] = ["desktop", "selection", "audio", "unavailable"];

test("expanded declarations are unique shallow preview data, not invocation contracts", () => {
  expect(catalog.status).toBe("prototype-fixtures-not-execution-contracts");
  expect(catalog.items).toHaveLength(96);
  expect(catalog.branches).toHaveLength(12);
  const declarations = [...catalog.items, ...catalog.branches];
  expect(new Set(declarations.map(item => item.id)).size).toBe(declarations.length);
  const leaves = new Set(catalog.items.map(item => item.id));
  const grouped = catalog.branches.flatMap(branch => branch.children);
  expect(new Set(grouped).size).toBe(grouped.length);
  for (const branch of catalog.branches) {
    expect(branch.node).not.toBe("root");
    expect(branch.children.every(id => leaves.has(id))).toBe(true);
  }
  for (const declaration of declarations) {
    for (const forbidden of ["command", "exec", "grant", "permission", "endpoint", "script"]) {
      expect(forbidden in declaration).toBe(false);
    }
  }
});

test("system controls explicitly name conditional mechanisms on every declared platform", () => {
  for (const item of catalog.items) {
    if (!["System", "Omarchy", "Hyprland"].includes(item.tool)) continue;
    const target = item as { platforms?: string[]; support?: Record<string, string> };
    expect(Object.keys(target.support || {}).sort()).toEqual([...(target.platforms || platforms)].sort());
    for (const condition of Object.values(target.support || {})) expect(condition.length).toBeGreaterThan(3);
  }
  expect(catalog.items.find(item => item.id === "sound.apps")?.support?.macos).toContain("no standard native mixer");
  expect(catalog.items.find(item => item.id === "display.external_brightness")?.support?.omarchy).toContain("brightnessctl alone is insufficient");
});

test("all platform/context scopes retain the entire eligible catalog within worker bounds", () => {
  for (const platform of platforms) for (const context of contexts) {
    const flat = candidates(platform, context);
    const root = candidates(platform, context, "branches");
    expect(flat.length).toBeGreaterThan(80);
    expect(flat.length).toBeLessThanOrEqual(128);
    expect(root.length).toBeLessThanOrEqual(128);
    const reachable = root.flatMap(item => item.kind === "branch" ? candidates(platform, context, "branches", item.node) : [item]);
    expect(new Set(reachable.map(item => item.id))).toEqual(new Set(flat.map(item => item.id)));
    for (const branch of catalog.branches) {
      const child = candidates(platform, context, "branches", branch.node);
      expect(child.length).toBeLessThanOrEqual(128);
      expect(child.every(item => !item.kind && !item.node)).toBe(true);
    }
    expect(flat.some(item => item.id === "theme.pick")).toBe(platform === "omarchy");
    expect(flat.some(item => item.id === "network.airplane")).toBe(platform !== "macos");
    expect(flat.some(item => item.id === "privacy.screen")).toBe(platform !== "windows");
    expect(flat.some(item => item.id === "appearance.auto")).toBe(platform !== "omarchy");
    expect(flat.some(item => item.id === "screenshot.capture")).toBe(context !== "unavailable");
  }
});

test("Jev sees conditions and refusal; adding controls never turns context into authority", () => {
  const body = payload("mute my microphone", "macos", "desktop", "jev-latest");
  expect(body.questions.route.criteria["sound.microphone_mute"].desc).toContain("No universal system-wide input mute switch");
  expect(body.questions.route.instructions.rules.join(" ")).toContain("not discovered host capabilities");
  expect(body.questions.route.criteria.REFUSE).toBeDefined();
});
