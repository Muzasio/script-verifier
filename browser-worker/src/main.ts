import { createInterface } from "node:readline";
import type { PageHandle } from "./browser.js";
import { BrowserManager } from "./browser.js";
import { loadConfig, type Config } from "./config.js";
import { clearClaudeChats, clearGeminiChats, clearDeepSeekChats, diagnoseProvider } from "./cleanup.js";
import { ClaudeAdapter } from "./adapters/claude.js";
import { GeminiAdapter } from "./adapters/gemini.js";
import { DeepSeekAdapter } from "./adapters/deepseek.js";
import type { IpcRequest, IpcResponse, LlmAdapter, Provider } from "./adapters/types.js";

// Fixed tab pool per provider. Tab count stays constant
// regardless of how many claims the orchestrator sends.
const POOL_SIZE = 3;

const browser = new BrowserManager();
const adapters = new Map<Provider, LlmAdapter>();
let config: Config;

function writeResponse(res: IpcResponse): void {
  process.stdout.write(JSON.stringify(res) + "\n");
}

function makeAdapter(provider: Provider): LlmAdapter {
  switch (provider) {
    case "claude":
      return new ClaudeAdapter(config.timeouts_ms.response_wait, config.retries);
    case "gemini":
      return new GeminiAdapter(config.timeouts_ms.response_wait, config.retries);
    case "deepseek":
      return new DeepSeekAdapter(config.timeouts_ms.response_wait, config.retries);
  }
}

function getAdapter(provider: Provider): LlmAdapter {
  let a = adapters.get(provider);
  if (!a) {
    a = makeAdapter(provider);
    adapters.set(provider, a);
  }
  return a;
}

async function handleRequest(req: IpcRequest): Promise<IpcResponse> {
  switch (req.action) {
    case "new_page": {
      const adapter = getAdapter(req.provider);
      try {
        const handle = await browser.newPage(req.provider, adapter, config.timeouts_ms.page_load);
        return { req_id: req.req_id, status: "ok", text: handle.pageId };
      } catch (e) {
        return { req_id: req.req_id, status: "error", message: `new_page failed: ${(e as Error).message}` };
      }
    }

    case "close_page": {
      await browser.closePage(req.page_id);
      return { req_id: req.req_id, status: "ok", text: "" };
    }

    case "health_check": {
      const handle = browser.getPage(req.page_id);
      if (!handle) {
        return { req_id: req.req_id, status: "error", message: `unknown page_id: ${req.page_id}` };
      }
      if (handle.page.isClosed()) {
        return { req_id: req.req_id, status: "error", message: "page is closed" };
      }
      try {
        const ready = await handle.adapter.isReady(handle.page);
        if (ready) {
          return { req_id: req.req_id, status: "ok", text: "ready" };
        }
        return { req_id: req.req_id, status: "error", message: "page not ready — likely logged out or still loading" };
      } catch (e) {
        return { req_id: req.req_id, status: "error", message: `health_check failed: ${(e as Error).message}` };
      }
    }

    case "send_prompt": {
      const adapter = getAdapter(req.provider);
      let handle: PageHandle | undefined;
      try {
        handle = await browser.pickFromPool(
          req.provider, adapter, POOL_SIZE, config.timeouts_ms.page_load,
        );
      } catch (e) {
        return { req_id: req.req_id, status: "error", message: `could not open pool page: ${(e as Error).message}` };
      }
      try {
        const text = await handle.runExclusive(() => handle!.adapter.sendPrompt(handle!.page, req.prompt));
        return { req_id: req.req_id, status: "ok", text };
      } catch (e) {
        return { req_id: req.req_id, status: "error", message: (e as Error).message ?? String(e) };
      }
    }

    case "clear_chats": {
      const adapter = getAdapter(req.provider);
      let handle: PageHandle | undefined;
      try {
        // Fresh non-pool page so cleanup doesn't disturb any running run.
        handle = await browser.newPage(req.provider, adapter, config.timeouts_ms.page_load);
      } catch (e) {
        return { req_id: req.req_id, status: "error", message: `could not open cleanup page: ${(e as Error).message}` };
      }
      try {
        const fn =
          req.provider === "claude" ? clearClaudeChats :
          req.provider === "gemini" ? clearGeminiChats :
          clearDeepSeekChats;
        const deleted = await fn(handle.page);
        await browser.closePage(handle.pageId).catch(() => {});
        return { req_id: req.req_id, status: "ok", text: String(deleted) };
      } catch (e) {
        await browser.closePage(handle.pageId).catch(() => {});
        return { req_id: req.req_id, status: "error", message: `cleanup failed: ${(e as Error).message}` };
      }
    }

    case "diagnose_page": {
      const adapter = getAdapter(req.provider);
      const urls: Record<string, string> = {
        claude: "https://claude.ai/new",
        gemini: "https://gemini.google.com/app",
        deepseek: "https://chat.deepseek.com/",
      };
      const url = urls[req.provider];
      if (!url) {
        return { req_id: req.req_id, status: "error", message: `unknown provider ${req.provider}` };
      }
      let handle: PageHandle | undefined;
      try {
        handle = await browser.newPage(req.provider, adapter, config.timeouts_ms.page_load);
        const diag = await diagnoseProvider(handle.page, req.provider, url);
        await browser.closePage(handle.pageId).catch(() => {});
        // Return a compact summary the UI can display.
        const lines: string[] = [];
        lines.push(`provider=${diag.provider}`);
        lines.push(`screenshot=${diag.screenshotPath}`);
        lines.push(`headings=${diag.headings.length}`);
        lines.push(`buttons=${diag.buttons.length}`);
        lines.push(`links=${diag.linksSample.length}`);
        lines.push("");
        lines.push("VISIBLE BUTTONS (text | aria-label):");
        for (const b of diag.buttons.filter((x: { visible: boolean }) => x.visible).slice(0, 40)) {
          lines.push(`  ${JSON.stringify(b.text)} | ${JSON.stringify(b.ariaLabel)}`);
        }
        lines.push("");
        lines.push("HEADINGS:");
        for (const h of diag.headings.slice(0, 20)) lines.push(`  ${JSON.stringify(h)}`);
        lines.push("");
        lines.push("LINK SAMPLE:");
        for (const l of diag.linksSample.slice(0, 20)) lines.push(`  ${JSON.stringify(l.href)} -> ${JSON.stringify(l.text)}`);
        return { req_id: req.req_id, status: "ok", text: lines.join("\n") };
      } catch (e) {
        const msg = e instanceof Error ? e.message : JSON.stringify(e);
        await browser.closePage(handle?.pageId ?? "").catch(() => {});
        return { req_id: req.req_id, status: "error", message: `diagnose failed: ${msg}` };
      }
    }

    default: {
      const a = (req as { action?: string }).action ?? "<missing>";
      return { req_id: req.req_id, status: "error", message: `unknown action: ${a}` };
    }
  }
}

async function main(): Promise<void> {
  config = await loadConfig();

  await browser.launch(config.browser_profile_dir);
  console.error(`[worker] ready — profile_dir=${config.browser_profile_dir} retries=${config.retries} response_wait=${config.timeouts_ms.response_wait}ms`);

  const rl = createInterface({ input: process.stdin, terminal: false });

  rl.on("line", (line) => {
    const trimmed = line.trim();
    if (!trimmed) return;

    let req: IpcRequest;
    try {
      req = JSON.parse(trimmed) as IpcRequest;
    } catch {
      writeResponse({ req_id: "unknown", status: "error", message: "invalid JSON request" });
      return;
    }

    void handleRequest(req)
      .then(writeResponse)
      .catch((e) => {
        writeResponse({ req_id: req.req_id, status: "error", message: (e as Error)?.message ?? String(e) });
      });
  });

  rl.on("close", () => void shutdown("stdin closed"));
}

let shuttingDown = false;
async function shutdown(reason: string): Promise<void> {
  if (shuttingDown) return;
  shuttingDown = true;
  console.error(`[worker] shutting down: ${reason}`);
  try {
    await browser.shutdown();
  } catch (e) {
    console.error(`[worker] shutdown error: ${(e as Error).message}`);
  }
  process.exit(0);
}

process.on("SIGINT", () => void shutdown("SIGINT"));
process.on("SIGTERM", () => void shutdown("SIGTERM"));

main().catch((e) => {
  console.error(`[worker] fatal: ${(e as Error).stack ?? e}`);
  process.exit(1);
});
