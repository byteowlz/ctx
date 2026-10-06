import { randomBytes, timingSafeEqual } from "node:crypto";
import { resolve } from "node:path";
import { mkdir, open, rename, rm } from "node:fs/promises";
import { branches, candidates, fixtures, items, payload, platforms, suggestions, type Platform, type ContextFixture, type Presentation } from "./decision";

export type Settings = {
  origin: string; reviewKey: string;
  endpoint: string; model: string; listenerToken: string; virtualKey: string;
};
const responseHeaders = {
  "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff", "Referrer-Policy": "no-referrer",
  "Content-Security-Policy": "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
};
function json(body: unknown, status = 200) {
  return Response.json(body, { status, headers: responseHeaders });
}
function matchesKey(value: string | null, expected: string) {
  if (!value || !expected) return false;
  const a = Buffer.from(value), b = Buffer.from(expected);
  return a.length === b.length && timingSafeEqual(a, b);
}
async function readBounded(message: Request | Response, limit: number, signal: AbortSignal): Promise<string> {
  if (Number(message.headers.get("content-length") || 0) > limit) throw new Error("too_large");
  const reader = message.body?.getReader();
  if (!reader || signal.aborted) throw new Error("invalid_body");
  const onAbort = () => { void reader.cancel().catch(() => {}); };
  signal.addEventListener("abort", onAbort, { once: true });
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (signal.aborted) throw new Error("aborted");
      if (done) break;
      length += value.length;
      if (length > limit) { await reader.cancel(); throw new Error("too_large"); }
      chunks.push(value);
    }
    return Buffer.concat(chunks).toString();
  } finally { signal.removeEventListener("abort", onAbort); reader.releaseLock(); }
}
async function decide(request: Request, settings: Settings, upstream: typeof fetch, now: number) {
  const abort = new AbortController();
  const onCancel = () => abort.abort();
  if (request.signal.aborted) abort.abort();
  request.signal.addEventListener("abort", onCancel, { once: true });
  let timeout = setTimeout(() => abort.abort(), 3000);
  try {
    let input;
    try {
      if (!request.headers.get("content-type")?.startsWith("application/json")) throw new Error("content_type");
      input = JSON.parse(await readBounded(request, 4096, abort.signal));
    } catch { return json({ error: "Use a JSON request of at most 4 KB" }, 400); }
    if (!input || typeof input !== "object" || Array.isArray(input)) return json({ error: "Use a JSON request object" }, 400);
    const { query, platform, fixture, presentation = "flat", node = "root" } = input as {
      query?: unknown; platform?: unknown; fixture?: unknown; presentation?: unknown; node?: unknown;
    };
    if (typeof query !== "string" || !query.trim() || Array.from(query).length > 512 ||
        typeof platform !== "string" || !platforms.includes(platform as Platform) ||
        typeof fixture !== "string" || !Object.hasOwn(fixtures, fixture) ||
        (presentation !== "flat" && presentation !== "branches") || typeof node !== "string") {
      return json({ error: "Choose a platform/fixture and enter 1–512 characters" }, 400);
    }
    let offered;
    try { offered = candidates(platform as Platform, fixture as ContextFixture, presentation as Presentation, node); }
    catch { return json({ error: "Choose a declared branch; depth is limited to root plus one child level" }, 400); }
    clearTimeout(timeout); timeout = setTimeout(() => abort.abort(), 15_000);
    const res = await upstream(settings.endpoint, {
      method: "POST", signal: abort.signal, redirect: "error",
      headers: { "Content-Type": "application/json", "X-Eavs-Token": settings.listenerToken, Authorization: `Bearer ${settings.virtualKey}` },
      body: JSON.stringify(payload(query.trim(), platform as Platform, fixture as ContextFixture, settings.model, presentation as Presentation, node)),
    });
    if (!res.ok) {
      await res.body?.cancel();
      return json({ error: `EAVS decision unavailable (HTTP ${res.status}). No fallback decision was invented.` }, 502);
    }
    const raw = await readBounded(res, 128_000, abort.signal);
    const result = suggestions(JSON.parse(raw), offered);
    return json({ items: result, source: "Jev via EAVS", latencyMs: Date.now() - now, fixtureOnly: true });
  } catch { return json({ error: "Jev request timed out or returned an invalid decision. Try again." }, 502); }
  finally { clearTimeout(timeout); request.signal.removeEventListener("abort", onCancel); }
}

export function handler(settings: Settings, upstream: typeof fetch = fetch) {
  let active = 0;
  let requests: number[] = [];
  return async (request: Request) => {
    const url = new URL(request.url);
    if (url.origin !== settings.origin) return json({ error: "Unexpected host" }, 403);
    if (url.pathname === "/api/catalog" && request.method === "GET") {
      return json({ platforms, items, branches, ready: Boolean(settings.virtualKey && settings.listenerToken), fixtureOnly: true, model: "Jev via EAVS" });
    }
    if (url.pathname !== "/api/suggest") return json({ error: "Not found" }, 404);
    if (request.method !== "POST") return json({ error: "Use POST" }, 405);
    if (request.headers.get("origin") !== settings.origin || !matchesKey(request.headers.get("x-prototype-key"), settings.reviewKey)) {
      return json({ error: "The private local prototype connection is required" }, 403);
    }
    if (!settings.virtualKey || !settings.listenerToken) return json({ error: "EAVS credentials unavailable" }, 503);
    const now = Date.now();
    requests = requests.filter(time => now - time < 60_000);
    if (active >= 2 || requests.length >= 12) return json({ error: "Request limit reached; wait a moment" }, 429);
    requests.push(now); active++;
    try { return await decide(request, settings, upstream, now); }
    finally { active--; }
  };
}

async function readToml(path: string): Promise<Record<string, any>> {
  return await Bun.file(path).exists() ? Bun.TOML.parse(await Bun.file(path).text()) : {};
}
export async function loadSettings(): Promise<Settings> {
  const configDir = resolve(process.env.XDG_CONFIG_HOME || `${process.env.HOME}/.config`, "ctx");
  const configPath = process.env.OMNIBAR_CONFIG || resolve(configDir, "omnibar-prototype.toml");
  if (!await Bun.file(configPath).exists()) {
    await mkdir(resolve(configPath, ".."), { recursive: true });
    await Bun.write(configPath, '#:schema https://raw.githubusercontent.com/byteowlz/ctx/main/examples/omnibar-prototype/config.schema.json\n# Local preview adapter; never put credentials in git.\nendpoint = "http://127.0.0.1:3033/jev/v1/systemone"\nmodel = "jev-latest"\n# eavs_profile = "an-existing-authorized-profile"\n# listener_token = ""\n# virtual_key = ""\n');
  }
  const config = await readToml(configPath);
  const eavsPath = process.env.OMNIBAR_EAVS_CONFIG || resolve(process.env.XDG_CONFIG_HOME || `${process.env.HOME}/.config`, "eavs/config.toml");
  const needsListener = !process.env.OMNIBAR_EAVS_TOKEN && !config.listener_token && !process.env.EAVS_AUTH_TOKEN;
  const eavs = needsListener ? await readToml(eavsPath) : {};
  const profile = process.env.OMNIBAR_EAVS_PROFILE || String(config.eavs_profile || "");
  let profileKey = "";
  if (profile && !process.env.OMNIBAR_EAVS_KEY && !config.virtual_key) {
    const profileFile = process.env.OMNIBAR_MODELS_FILE || resolve(process.env.HOME || "", ".pi/agent/models.json");
    const profiles = await Bun.file(profileFile).json();
    profileKey = profiles.providers?.[profile]?.apiKey || "";
    if (!profileKey) throw new Error("The explicitly selected local EAVS profile is unavailable");
  }
  const host = process.env.OMNIBAR_HOST || "127.0.0.1";
  const port = Number(process.env.OMNIBAR_PORT || 4784);
  if (host !== "127.0.0.1" || !Number.isInteger(port) || port < 1024 || port > 65535) throw new Error("Native prototype must bind IPv4 loopback on a non-privileged port");
  const settings: Settings = {
    origin: `http://${host}:${port}`, reviewKey: process.env.OMNIBAR_REVIEW_KEY || randomBytes(24).toString("hex"),
    endpoint: process.env.OMNIBAR_EAVS_URL || String(config.endpoint || "http://127.0.0.1:3033/jev/v1/systemone"),
    model: process.env.OMNIBAR_EAVS_MODEL || String(config.model || "jev-latest"),
    listenerToken: process.env.OMNIBAR_EAVS_TOKEN || String(config.listener_token || "") || process.env.EAVS_AUTH_TOKEN || eavs.server?.auth_token || eavs.keys?.master_key || "",
    virtualKey: process.env.OMNIBAR_EAVS_KEY || String(config.virtual_key || "") || profileKey,
  };
  const endpoint = new URL(settings.endpoint);
  if (endpoint.protocol !== "http:" || endpoint.hostname !== "127.0.0.1" || endpoint.username || endpoint.password) throw new Error("Prototype upstream must be the local EAVS listener");
  return settings;
}
export async function publishConnection(settings: Settings) {
  const stateDir = resolve(process.env.XDG_STATE_HOME || `${process.env.HOME}/.local/state`, "ctx");
  await mkdir(stateDir, { recursive: true, mode: 0o700 });
  const path = resolve(stateDir, "omnibar-prototype.json");
  const temp = `${path}.${randomBytes(8).toString("hex")}.tmp`;
  const file = await open(temp, "wx", 0o600);
  try { await file.writeFile(JSON.stringify({ origin: settings.origin, reviewKey: settings.reviewKey })); }
  finally { await file.close(); }
  try { await rename(temp, path); }
  finally { await rm(temp, { force: true }); }
  return path;
}

if (import.meta.main) {
  let settings;
  try { settings = await loadSettings(); }
  catch { console.error("Cannot read the local prototype configuration or explicitly selected EAVS profile. No credentials were printed."); process.exit(1); }
  const port = new URL(settings.origin).port;
  const server = Bun.serve({ hostname: "127.0.0.1", port: Number(port), idleTimeout: 30, fetch: handler(settings) });
  const descriptor = await publishConnection(settings);
  console.log(`Native ctx adapter: ${settings.origin}; EAVS ${settings.virtualKey && settings.listenerToken ? "configured" : "credentials unavailable"}`);
  console.log("Synthetic context only. Selections do not execute actions. Connection credentials are in the private local state descriptor, not browser assets.");
  const stop = async () => {
    server.stop();
    if (await Bun.file(descriptor).exists() && (await Bun.file(descriptor).json()).reviewKey === settings.reviewKey) await rm(descriptor, { force: true });
    process.exit(0);
  };
  process.on("SIGINT", stop); process.on("SIGTERM", stop);
}
