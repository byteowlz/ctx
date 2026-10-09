import { isAbsolute, resolve } from "node:path";
import { configBase, readToml } from "./config";

export type RetrievalConfig = { enabled: boolean; backend: "cpu" | "ane"; worker_path: string; model_dir: string };
const keys = ["enabled", "backend", "worker_path", "model_dir"];
function layer(document: Record<string, any>): Partial<RetrievalConfig> {
  const value = document.retrieval ?? {};
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Retrieval settings must be a table");
  for (const [key, setting] of Object.entries(value)) {
    if (!keys.includes(key)) throw new Error("Unknown retrieval setting");
    if (key === "enabled" ? typeof setting !== "boolean" : typeof setting !== "string") throw new Error("Invalid retrieval setting type");
  }
  return value;
}
export async function loadRetrievalConfig(env: NodeJS.ProcessEnv = process.env, cwd = process.cwd()): Promise<RetrievalConfig> {
  const home = env.HOME || env.USERPROFILE;
  if (!home) throw new Error("HOME is required");
  const cache = env.XDG_CACHE_HOME && isAbsolute(env.XDG_CACHE_HOME) ? env.XDG_CACHE_HOME : resolve(home, ".cache");
  let result: RetrievalConfig = {
    enabled: false, backend: "cpu",
    worker_path: resolve(cache, "ctx/retrieval-runtime/worker"), model_dir: resolve(cache, "ctx/retrieval-probe/model"),
    ...layer(await readToml(resolve(configBase(env), "ctx/config.toml"))),
    ...layer(await readToml(resolve(cwd, "ctx.toml"))),
  };
  const overrides: Partial<RetrievalConfig> = {};
  if (env.CTX__RETRIEVAL__ENABLED !== undefined) {
    if (!["true", "false"].includes(env.CTX__RETRIEVAL__ENABLED)) throw new Error("Invalid retrieval enabled override");
    overrides.enabled = env.CTX__RETRIEVAL__ENABLED === "true";
  }
  for (const key of ["backend", "worker_path", "model_dir"] as const) {
    const value = env[`CTX__RETRIEVAL__${key.toUpperCase()}`];
    if (value !== undefined) (overrides as Record<string, string>)[key] = value;
  }
  result = { ...result, ...overrides };
  const explicit = env.CTX_CONFIG || env.OMNIBAR_CONFIG;
  if (explicit) {
    if (!await Bun.file(explicit).exists()) throw new Error("Explicit ctx configuration is unavailable");
    result = { ...result, ...layer(await readToml(explicit)) };
  }
  if (!["cpu", "ane"].includes(result.backend) || !isAbsolute(result.worker_path) || !isAbsolute(result.model_dir)) throw new Error("Retrieval requires cpu/ane and absolute asset paths");
  return result;
}
