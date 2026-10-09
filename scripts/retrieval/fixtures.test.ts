import { expect, test } from "bun:test";
import { fixtures } from "./fixtures";
const catalog = await Bun.file(new URL("../../examples/omnibar-prototype/catalog.json", import.meta.url)).json();
test("frozen synthetic fixtures have unique IDs, independent labels and exact sizes", () => {
  const a = fixtures(catalog), b = fixtures(catalog);
  expect(a).toEqual(b);
  expect(a.documents).toHaveLength(28); expect(a.distractors).toHaveLength(972);
  expect(new Set([...a.documents, ...a.distractors].map(doc => doc.id)).size).toBe(1000);
  expect(a.queries.filter(q => q.group === "literal")).toHaveLength(28);
  expect(a.queries.filter(q => q.group === "paraphrase")).toHaveLength(56);
  expect(a.queries.filter(q => q.group === "no-match")).toHaveLength(8);
  expect(a.queries.filter(q => q.id.startsWith("selection.speak")).every(q => q.fixture === "selection")).toBe(true);
  expect(a.queries.filter(q => q.id.startsWith("audio.transcribe")).every(q => q.fixture === "audio")).toBe(true);
  expect(a.queries.filter(q => q.id.startsWith("theme.pick")).every(q => q.platform === "omarchy")).toBe(true);
  expect(a.fixtureSha256).toMatch(/^[a-f0-9]{64}$/);
});
