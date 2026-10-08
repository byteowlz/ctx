import { open, chmod, mkdir } from "node:fs/promises";
import { isAbsolute, resolve } from "node:path";

export type EavsConfig = {
  endpoint?: string; model?: string; profile?: string;
  listener_token?: string; virtual_key?: string;
};
const fields = ["endpoint", "model", "profile", "listener_token", "virtual_key"];
const defaults: EavsConfig = { endpoint: "http://127.0.0.1:3033/jev/v1/systemone", model: "jev-latest" };
const header = "#:schema https://raw.githubusercontent.com/byteowlz/schemas/refs/heads/main/ctx/ctx.config.schema.json\n";

export async function readToml(path: string): Promise<Record<string, any>> {
  return await Bun.file(path).exists() ? Bun.TOML.parse(await Bun.file(path).text()) : {};
}
function section(document: Record<string, any>, legacy = false): EavsConfig {
  const shared = ["bar", "capture", "providers", "output", "ocr"].some(key => Object.hasOwn(document, key));
  const value = document.eavs ?? (legacy && !shared ? document : {});
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("EAVS settings must be a table");
  const config = { ...value };
  if (legacy && config.eavs_profile !== undefined) {
    config.profile = config.eavs_profile; delete config.eavs_profile;
  }
  for (const [key, value] of Object.entries(config)) {
    if (!fields.includes(key) || typeof value !== "string" || (["endpoint", "model"].includes(key) && !value)) throw new Error("Invalid EAVS setting");
  }
  return config;
}
export async function ensureEavsConfig(path: string, legacyPath: string, coreLegacyPath?: string): Promise<void> {
  await mkdir(resolve(path, ".."), { recursive: true, mode: 0o700 });
  if (!await Bun.file(path).exists()) {
    const content = coreLegacyPath && await Bun.file(coreLegacyPath).exists() ? await Bun.file(coreLegacyPath).text() : header;
    Bun.TOML.parse(content);
    try {
      const file = await open(path, "wx", 0o600);
      try { await file.writeFile(content); } finally { await file.close(); }
    } catch (error) { if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error; }
  }
  const existing = await readToml(path);
  if (Object.hasOwn(existing, "eavs")) { section(existing); return; }
  const migrated = await Bun.file(legacyPath).exists() ? section(await readToml(legacyPath), true) : {};
  const config = { ...defaults, ...migrated };
  const content = "\n[eavs]\n" + Object.entries(config).map(([key, value]) => `${key} = ${JSON.stringify(value)}\n`).join("");
  if (config.listener_token || config.virtual_key) await chmod(path, 0o600);
  const file = await open(path, "a", 0o600);
  try { await file.writeFile(content); } finally { await file.close(); }
}
export function configBase(env: NodeJS.ProcessEnv): string {
  const home = env.HOME || env.USERPROFILE;
  if (!home) throw new Error("HOME or USERPROFILE is required");
  return env.XDG_CONFIG_HOME && isAbsolute(env.XDG_CONFIG_HOME) ? env.XDG_CONFIG_HOME : resolve(home, ".config");
}
export async function loadEavsConfig(env: NodeJS.ProcessEnv = process.env, cwd = process.cwd()): Promise<EavsConfig> {
  const home = env.HOME || env.USERPROFILE!;
  const dir = resolve(configBase(env), "ctx");
  const explicit = env.CTX_CONFIG || env.OMNIBAR_CONFIG;
  const path = resolve(dir, "config.toml");
  const coreLegacy = process.platform === "darwin" && path === resolve(home, ".config/ctx/config.toml") ? resolve(home, "Library/Application Support/ctx/config.toml") : undefined;
  if (!explicit) await ensureEavsConfig(path, resolve(dir, "omnibar-prototype.toml"), coreLegacy);
  let config: EavsConfig = { ...defaults, ...section(await readToml(path)), ...section(await readToml(resolve(cwd, "ctx.toml"))) };
  const overrides: EavsConfig = {
    endpoint: env.CTX__EAVS__ENDPOINT || env.CTX_EAVS__ENDPOINT || env.OMNIBAR_EAVS_URL,
    model: env.CTX__EAVS__MODEL || env.CTX_EAVS__MODEL || env.OMNIBAR_EAVS_MODEL,
    profile: env.CTX__EAVS__PROFILE || env.CTX_EAVS__PROFILE || env.OMNIBAR_EAVS_PROFILE,
    listener_token: env.CTX__EAVS__LISTENER_TOKEN || env.CTX_EAVS__LISTENER_TOKEN || env.OMNIBAR_EAVS_TOKEN,
    virtual_key: env.CTX__EAVS__VIRTUAL_KEY || env.CTX_EAVS__VIRTUAL_KEY || env.OMNIBAR_EAVS_KEY,
  };
  config = { ...config, ...Object.fromEntries(Object.entries(overrides).filter(([, value]) => value)) };
  if (explicit) {
    if (!await Bun.file(explicit).exists()) throw new Error("Explicit ctx configuration is unavailable");
    config = { ...config, ...section(await readToml(explicit), true) };
  }
  return config;
}
