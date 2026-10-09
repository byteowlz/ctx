import { resolve } from "node:path";
import type { Item } from "./decision";
import type { RetrievalConfig } from "./retrieval-config";
import pinnedAssets from "../../scripts/retrieval/assets.json";

export type RetrievalResult = { ids: string[]; source: "Local Evoke"; latencyMs: number; truncated: boolean };
export interface Retriever {
  readonly enabled: boolean;
  search(query: string, offered: Item[]): Promise<RetrievalResult>;
  close(): void;
}
export class EvokeWorker implements Retriever {
  readonly enabled: boolean;
  private process?: ReturnType<typeof Bun.spawn>;
  private ready: Promise<void>;
  private resolveReady!: () => void;
  private rejectReady!: (error: Error) => void;
  private serial = 0;
  private failed = false;
  private pending?: { id: number; resolve: (value: any) => void; reject: (error: Error) => void };
  constructor(private config: RetrievalConfig) {
    this.enabled = config.enabled;
    this.ready = new Promise((yes, no) => { this.resolveReady = yes; this.rejectReady = no; });
    // The availability state owns startup errors even before a search awaits readiness.
    void this.ready.catch(() => { this.failed = true; });
    if (this.enabled) void this.start();
    else this.resolveReady();
  }
  private fail(): void {
    this.failed = true;
    const error = new Error("Local retrieval unavailable; Jev is unchanged");
    this.rejectReady(error); this.pending?.reject(error); this.pending = undefined;
    this.process?.kill();
  }
  private async start(): Promise<void> {
    const timer = setTimeout(() => this.fail(), 10_000);
    try {
      if (process.platform !== "darwin" || process.arch !== "arm64") throw new Error("Unsupported backend");
      const manifest = resolve(this.config.worker_path, "../assets.json");
      if (!await Bun.file(this.config.worker_path).exists() || !await Bun.file(manifest).exists()) throw new Error("Install assets");
      const installed = await Bun.file(manifest).json();
      if (JSON.stringify(installed) !== JSON.stringify(pinnedAssets)) throw new Error("Unpinned asset manifest");
      if (this.failed) return;
      this.process = Bun.spawn([this.config.worker_path, this.config.model_dir, manifest, this.config.backend], {
        stdin: "pipe", stdout: "pipe", stderr: "ignore",
        env: { HOME: process.env.HOME || "", TMPDIR: process.env.TMPDIR || "/tmp", PATH: "/usr/bin:/bin" },
      });
      void this.process.exited.then(() => this.fail());
      await this.readOutput(clearTimeout.bind(null, timer));
    } catch { this.fail(); }
    finally { clearTimeout(timer); }
  }
  private acceptOutput(output: any, booted: boolean): boolean {
    if (!booted) {
      if (output.ready !== true || Object.keys(output).length !== 1) throw new Error("Invalid handshake");
      this.resolveReady(); return true;
    }
    if (!this.pending || output.id !== this.pending.id) throw new Error("Invalid response ticket");
    this.pending.resolve(output); this.pending = undefined;
    return true;
  }
  private async readOutput(onReady: () => void): Promise<void> {
    const stdout = this.process?.stdout;
    if (!stdout || typeof stdout === "number") throw new Error("Missing stdout");
    const reader = stdout.getReader(), decoder = new TextDecoder();
    let buffer = "", booted = false;
    try {
      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        if (buffer.length > 16_384) throw new Error("Output too large");
        while (buffer.includes("\n")) {
          const offset = buffer.indexOf("\n"), line = buffer.slice(0, offset); buffer = buffer.slice(offset + 1);
          const output = JSON.parse(line);
          if (!booted) onReady();
          booted = this.acceptOutput(output, booted);
        }
      }
    } finally { reader.releaseLock(); }
    this.fail();
  }
  async search(query: string, offered: Item[]): Promise<RetrievalResult> {
    if (!this.enabled) throw new Error("Local retrieval disabled");
    await this.ready;
    if (this.failed || this.pending || !this.process) throw new Error("Local retrieval busy or unavailable");
    if (!query.trim() || Array.from(query).length > 512 || offered.length > 128 || new Set(offered.map(item => item.id)).size !== offered.length) throw new Error("Invalid local input");
    const id = ++this.serial;
    const documents = offered.map(item => ({ id: item.id, text: `${item.label}. ${item.description}. ${item.tool} ${item.interface}` }));
    const line = JSON.stringify({ id, query, documents }) + "\n";
    if (Buffer.byteLength(line) > 512 * 1024) throw new Error("Retrieval input too large");
    const result = new Promise<any>((yes, no) => { this.pending = { id, resolve: yes, reject: no }; });
    const timeout = setTimeout(() => this.fail(), 3000);
    try {
      const stdin = this.process.stdin;
      if (!stdin || typeof stdin === "number") throw new Error("Missing stdin");
      stdin.write(line); await stdin.flush();
      const output = await result;
      if ((output.error && !output.truncated) || !Array.isArray(output.ids) || output.ids.length > 5 ||
          (output.truncated && output.ids.length !== 0) ||
          typeof output.truncated !== "boolean" || !Number.isFinite(output.latencyMs) || output.latencyMs < 0 ||
          new Set(output.ids).size !== output.ids.length || output.ids.some((candidate: unknown) => !offered.some(item => item.id === candidate))) throw new Error("Invalid local result");
      return { ids: output.ids, truncated: output.truncated, latencyMs: output.latencyMs, source: "Local Evoke" };
    } catch { throw new Error("Local retrieval unavailable; Jev is unchanged"); }
    finally { clearTimeout(timeout); }
  }
  close(): void { this.fail(); }
}
