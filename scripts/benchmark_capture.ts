import { mkdtemp, chmod, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";

// Capture data stays private and is deleted; stdout contains timings/counts only.
const binary = resolve(process.argv[2] || "target/release/ctx");
const runs = Number(process.argv[3] || 3);
if (!Number.isInteger(runs) || runs < 1 || runs > 30) throw new Error("Use 1–30 runs");
const captureDir = await mkdtemp(`${tmpdir()}/ctx-latency-`);
await chmod(captureDir, 0o700);
const currentFile = resolve(captureDir, "current.json");
await Bun.write(currentFile, JSON.stringify({
  schema: "https://byteowlz.github.io/schemas/ctx/current-context.v1.json", version: 1,
  updated_at: new Date().toISOString(), sequence: 1,
  active: { source: "benchmark", kind: "application", app: "Synthetic application" },
}));
const modes: [string, string[]][] = [
  ["full", ["--screenshots", "--accessibility"]],
  ["screenshots_only", ["--screenshots", "--no-accessibility"]],
  ["accessibility_only", ["--no-screenshots", "--accessibility"]],
  ["metadata_only", ["--no-screenshots", "--no-accessibility"]],
  ["noop", ["--provider", "noop", "--no-screenshots", "--no-accessibility"]],
  ["current_file", ["--state-file", currentFile, "current", "--json"]],
];
try {
  const fingerprint = async () => new Bun.CryptoHasher("sha256").update(await Bun.file(binary).arrayBuffer()).digest("hex");
  const binarySha256 = await fingerprint();
  const results: string[] = [];
  for (const [mode, extra] of modes) {
    const wallMs: number[] = [];
    let captureCount = 0;
    let accessibilityCaptured: boolean | null = null;
    const flags = mode === "current_file" ? extra : ["--json", "--no-clipboard", "--no-actions", "--capture-dir", captureDir, ...extra];
    for (let i = 0; i < runs; i++) {
      const start = performance.now();
      const child = Bun.spawn([binary, ...flags], { stdout: "pipe", stderr: "pipe" });
      const [text] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text()]);
      if (await child.exited !== 0) throw new Error(`Benchmark ${mode} failed; run the CLI directly for diagnostics`);
      wallMs.push(Number((performance.now() - start).toFixed(2)));
      if (mode !== "current_file") {
        const envelope = JSON.parse(text);
        captureCount = envelope.context.screenshots.captures.filter((capture: { path?: unknown }) => capture.path).length;
        accessibilityCaptured = envelope.context.accessibility.captured;
      }
    }
    const sorted = [...wallMs].sort((a, b) => a - b);
    const mid = Math.floor(sorted.length / 2);
    const medianMs = sorted.length % 2 ? sorted[mid] : (sorted[mid - 1]! + sorted[mid]!) / 2;
    results.push(JSON.stringify({ mode, wallMs, medianMs, captureCount, accessibilityCaptured, binarySha256 }));
  }
  if (await fingerprint() !== binarySha256) throw new Error("Executable changed during the benchmark; discard this run and retry");
  console.log(results.join("\n"));
} finally {
  await rm(captureDir, { recursive: true, force: true });
}
