import { expect, test } from "bun:test";
import { mkdtemp, readFile, rm, stat, writeFile, mkdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ensureEavsConfig, loadEavsConfig } from "./config";
import { loadSettings } from "./server";

async function fixture(run: (dir: string) => Promise<void>) {
  const dir = await mkdtemp(join(tmpdir(), "ctx-shared-config-"));
  try { await run(dir); } finally { await rm(dir, { recursive: true, force: true }); }
}

test("EAVS migration preserves capture/bar/comments and legacy backup; current table wins", async () => {
  await fixture(async dir => {
    const path = join(dir, "config.toml"), legacy = join(dir, "omnibar-prototype.toml");
    const original = "# keep\n[capture]\nimage_quality = 42\n[bar]\ndesign = 'corners'\n";
    const old = "endpoint = 'http://127.0.0.1:3034/jev/v1/systemone'\neavs_profile = 'synthetic-profile'\nvirtual_key = 'synthetic-key'\n";
    await writeFile(path, original); await writeFile(legacy, old);
    await ensureEavsConfig(path, legacy);
    const saved = await readFile(path, "utf8");
    expect(saved.startsWith(original)).toBe(true);
    expect(await readFile(legacy, "utf8")).toBe(old);
    expect(Bun.TOML.parse(saved).eavs).toEqual({ endpoint: "http://127.0.0.1:3034/jev/v1/systemone", model: "jev-latest", profile: "synthetic-profile", virtual_key: "synthetic-key" });
    expect((await stat(path)).mode & 0o777).toBe(0o600);
    await writeFile(legacy, "bad = [");
    await ensureEavsConfig(path, legacy);
    expect(await readFile(path, "utf8")).toBe(saved);
    await writeFile(path, original);
    await expect(ensureEavsConfig(path, legacy)).rejects.toThrow();
    expect(await readFile(path, "utf8")).toBe(original);
  });
});

test("shared global/local/environment/explicit precedence with isolated credential sources", async () => {
  await fixture(async dir => {
    const configDir = join(dir, "ctx"); await mkdir(configDir);
    const env = { HOME: dir, XDG_CONFIG_HOME: dir };
    await writeFile(join(configDir, "config.toml"), "[bar]\ndesign = 'corners'\n[eavs]\nmodel = 'global'\nprofile = 'global-profile'\n");
    await writeFile(join(dir, "ctx.toml"), "[eavs]\nmodel = 'local'\n");
    expect((await loadEavsConfig(env, dir)).model).toBe("local");
    expect((await loadEavsConfig({ ...env, CTX_EAVS__MODEL: "env" }, dir)).model).toBe("env");
    expect((await loadEavsConfig({ ...env, OMNIBAR_EAVS_MODEL: "legacy-env" }, dir)).model).toBe("legacy-env");
    const explicit = join(dir, "explicit.toml");
    await writeFile(explicit, "[capture]\nimage_quality = 43\n[eavs]\nmodel = 'explicit'\n");
    expect(await loadEavsConfig({ ...env, CTX_CONFIG: explicit, CTX_EAVS__MODEL: "env" }, dir)).toMatchObject({ model: "explicit", profile: "global-profile" });
    await writeFile(explicit, "eavs_profile = 'legacy-profile'\nmodel = 'legacy-explicit'\n");
    expect(await loadEavsConfig({ ...env, OMNIBAR_CONFIG: explicit }, dir)).toMatchObject({ model: "legacy-explicit", profile: "legacy-profile" });
    await writeFile(explicit, "[bar]\ndesign = 'corners'\n");
    expect((await loadEavsConfig({ ...env, CTX_CONFIG: explicit }, dir)).model).toBe("local");
    await writeFile(explicit, "[eavs]\nunknown = true\n");
    await expect(loadEavsConfig({ ...env, CTX_CONFIG: explicit }, dir)).rejects.toThrow();
    await expect(loadEavsConfig({ ...env, CTX_CONFIG: join(dir, "missing") }, dir)).rejects.toThrow();
  });
});

test("adapter first run uses private shared file, never prototype config; malformed shared file untouched", async () => {
  await fixture(async dir => {
    const path = join(dir, "ctx/config.toml");
    const env = { HOME: dir, XDG_CONFIG_HOME: dir };
    expect(await loadEavsConfig(env, dir)).toEqual({ endpoint: "http://127.0.0.1:3033/jev/v1/systemone", model: "jev-latest" });
    expect((await stat(path)).mode & 0o777).toBe(0o600);
    expect(await Bun.file(join(dir, "ctx/omnibar-prototype.toml")).exists()).toBe(false);
    const parsed = Bun.TOML.parse(await readFile(path, "utf8"));
    expect(parsed.eavs).toBeDefined(); expect(parsed.endpoint).toBeUndefined();
    await writeFile(path, "bad = [");
    await expect(loadEavsConfig(env, dir)).rejects.toThrow();
    expect(await readFile(path, "utf8")).toBe("bad = [");
  });
});

test("shared EAVS profile resolves existing private credentials, not another provider", async () => {
  await fixture(async dir => {
    await mkdir(join(dir, "ctx")); await mkdir(join(dir, "eavs")); await mkdir(join(dir, ".pi/agent"), { recursive: true });
    await writeFile(join(dir, "ctx/config.toml"), "[bar]\ndesign = 'corners'\n[eavs]\nprofile = 'synthetic-profile'\n");
    await writeFile(join(dir, "eavs/config.toml"), "[server]\nauth_token = 'synthetic-listener'\n");
    await writeFile(join(dir, ".pi/agent/models.json"), JSON.stringify({ providers: { "synthetic-profile": { apiKey: "synthetic-virtual" } } }));
    const env = { HOME: dir, XDG_CONFIG_HOME: dir };
    const settings = await loadSettings(env, dir);
    expect(settings).toMatchObject({ model: "jev-latest", endpoint: "http://127.0.0.1:3033/jev/v1/systemone", listenerToken: "synthetic-listener", virtualKey: "synthetic-virtual" });
    expect(settings.reviewKey).not.toContain("synthetic");
    const explicit = join(dir, "explicit.toml");
    await writeFile(explicit, "[eavs]\nlistener_token = 'synthetic-explicit-listener'\nvirtual_key = 'synthetic-explicit-key'\n");
    expect(await loadSettings({ ...env, CTX_CONFIG: explicit, CTX_EAVS__VIRTUAL_KEY: "synthetic-env-key", CTX_EAVS__LISTENER_TOKEN: "synthetic-env-listener", OMNIBAR_MODELS_FILE: "must-not-be-read" }, dir)).toMatchObject({ listenerToken: "synthetic-explicit-listener", virtualKey: "synthetic-explicit-key" });
    await writeFile(explicit, "[eavs]\nendpoint = 'http://remote.invalid/jev/v1/systemone'\nvirtual_key = 'synthetic-explicit-key'\n");
    await expect(loadSettings({ ...env, CTX_CONFIG: explicit }, dir)).rejects.toThrow("local EAVS listener");
  });
});

test("adapter-first migration retains old macOS core settings as an unchanged backup", async () => {
  await fixture(async dir => {
    const legacy = join(dir, "legacy-core.toml"), path = join(dir, "config.toml");
    const original = "# core legacy\n[capture]\nimage_quality = 43\n[providers.api_keys]\ntest = 'synthetic-key'\n";
    await writeFile(legacy, original);
    await ensureEavsConfig(path, join(dir, "missing-adapter.toml"), legacy);
    expect((await readFile(path, "utf8")).startsWith(original)).toBe(true);
    expect(await readFile(legacy, "utf8")).toBe(original);
    expect((await stat(path)).mode & 0o777).toBe(0o600);
    await writeFile(legacy, "broken = [");
    await ensureEavsConfig(path, join(dir, "missing-adapter.toml"), legacy);
    const before = await readFile(path, "utf8");
    await expect(ensureEavsConfig(join(dir, "another.toml"), join(dir, "missing-adapter.toml"), legacy)).rejects.toThrow();
    expect(await readFile(path, "utf8")).toBe(before);
  });
});
