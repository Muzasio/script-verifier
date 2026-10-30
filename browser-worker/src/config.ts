import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { resolve } from "node:path";

export interface Config {
  providers: string[];
  roles: {
    research: string;
    challenge: string;
    recheck: string;
    final: string;
  };
  browser_profile_dir: string;
  timeouts_ms: { page_load: number; response_wait: number };
  retries: number;
  source_priority: string[];
}

const DEFAULTS: Config = {
  providers: ["gemini", "claude", "deepseek"],
  roles: { research: "gemini", challenge: "claude", recheck: "gemini", final: "claude" },
  browser_profile_dir: "./profile",
  timeouts_ms: { page_load: 30_000, response_wait: 60_000 },
  retries: 3,
  source_priority: ["rfc_standard", "official_docs", "paper", "reputable_technical", "other"],
};

export async function loadConfig(): Promise<Config> {
  const candidates = ["config.json", "../config.json", "../../config.json"];
  for (const c of candidates) {
    const p = resolve(process.cwd(), c);
    if (!existsSync(p)) continue;
    try {
      const raw = await readFile(p, "utf8");
      const parsed = JSON.parse(raw) as Partial<Config>;
      return {
        ...DEFAULTS,
        ...parsed,
        timeouts_ms: { ...DEFAULTS.timeouts_ms, ...(parsed.timeouts_ms ?? {}) },
        roles: { ...DEFAULTS.roles, ...(parsed.roles ?? {}) },
      };
    } catch (e) {
      console.error(`[config] failed to parse ${p}: ${(e as Error).message}`);
    }
  }
  console.error("[config] no config.json found, using defaults");
  return DEFAULTS;
}
