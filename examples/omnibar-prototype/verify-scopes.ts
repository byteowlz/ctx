// Compare GPUI-free Rust declarations with the adapter, including stable order.
import assert from "node:assert/strict";
import { candidates, payload, type ContextFixture, type Platform, type Presentation } from "./decision";

const path = process.argv[2];
if (!path) throw new Error("usage: bun verify-scopes.ts ABSOLUTE_RUST_JSON_PATH");
const scopes: { platform: Platform; fixture: ContextFixture; presentation: Presentation; node: string; ids: string[]; payload: unknown }[] = await Bun.file(path).json();
assert.equal(scopes.length, 224);
for (const scope of scopes) {
  assert.deepEqual(scope.ids, candidates(scope.platform, scope.fixture, scope.presentation, scope.node).map(item => item.id), JSON.stringify(scope.ids));
  // The native direct transport sends exactly the adapter's decision request.
  assert.deepEqual(scope.payload, payload("synthetic scope proof", scope.platform, scope.fixture, "jev-latest", scope.presentation, scope.node));
}
console.log(`Rust/adapter parity: ${scopes.length} platform/context/presentation/node scopes and decision payloads; no host capability or execution proof.`);
