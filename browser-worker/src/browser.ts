import { chromium, type BrowserContext, type Page } from "playwright";
import type { LlmAdapter, Provider } from "./adapters/types.js";

export class PageHandle {
  private tail: Promise<unknown> = Promise.resolve();

  constructor(
    public readonly pageId: string,
    public readonly provider: Provider,
    public readonly adapter: LlmAdapter,
    public readonly page: Page,
  ) {}

  pendingCount(): number {
    return (this as any).__pending ?? 0;
  }

  runExclusive<T>(fn: () => Promise<T>): Promise<T> {
    (this as any).__pending = ((this as any).__pending ?? 0) + 1;
    const run = this.tail.then(fn, fn).finally(() => {
      (this as any).__pending = Math.max(0, ((this as any).__pending ?? 1) - 1);
    });
    this.tail = run.catch(() => undefined);
    return run;
  }
}

export class BrowserManager {
  private context: BrowserContext | null = null;
  private readonly pages = new Map<string, PageHandle>();
  private readonly counters = new Map<Provider, number>();

  async launch(profileDir: string): Promise<void> {
    this.context = await chromium.launchPersistentContext(profileDir, {
      headless: false,
      viewport: { width: 1280, height: 900 },
      args: [
        "--disable-blink-features=AutomationControlled",
        "--no-first-run",
        "--no-default-browser-check",
      ],
      ignoreDefaultArgs: ["--enable-automation"],
    });
  }

  async newPage(provider: Provider, adapter: LlmAdapter, pageLoadTimeoutMs: number,
    explicitPageId?: string,
  ): Promise<PageHandle> {
    if (!this.context) throw new Error("browser not launched");

    let pageId = explicitPageId;
    if (!pageId) {
      const n = (this.counters.get(provider) ?? 0) + 1;
      this.counters.set(provider, n);
      pageId = `${provider}-${n}`;
    }

    const page = await this.context.newPage();
    await page.goto(adapter.url, { waitUntil: "domcontentloaded", timeout: pageLoadTimeoutMs });

    const deadline = Date.now() + pageLoadTimeoutMs;
    let ready = false;
    while (Date.now() < deadline) {
      if (await adapter.isReady(page).catch(() => false)) {
        ready = true;
        break;
      }
      await page.waitForTimeout(500);
    }
    if (!ready) {
      console.error(`[browser] page ${pageId} not ready after ${pageLoadTimeoutMs}ms — send_prompt will surface a precise error`);
    }

    const handle = new PageHandle(pageId, provider, adapter, page);
    this.pages.set(pageId, handle);
    return handle;
  }

  /**
   * Audit fix 4e: the Rust orchestrator's `page_id` (e.g. "claude-c-0001")
   * is a stable handle for a claim's conversation thread — it never calls
   * `new_page` first. This creates the page on first use under that exact
   * id instead of requiring the caller to pre-register it.
   */
  async ensurePage(
    pageId: string,
    provider: Provider,
    adapter: LlmAdapter,
    pageLoadTimeoutMs: number,
  ): Promise<PageHandle> {
    const existing = this.pages.get(pageId);
    if (existing) return existing;
    if (!this.context) throw new Error("browser not launched");

    const page = await this.context.newPage();
    await page.goto(adapter.url, { waitUntil: "domcontentloaded", timeout: pageLoadTimeoutMs });

    const deadline = Date.now() + pageLoadTimeoutMs;
    let ready = false;
    while (Date.now() < deadline) {
      if (await adapter.isReady(page).catch(() => false)) {
        ready = true;
        break;
      }
      await page.waitForTimeout(500);
    }
    if (!ready) {
      console.error(`[browser] page ${pageId} not ready after ${pageLoadTimeoutMs}ms`);
    }

    const handle = new PageHandle(pageId, provider, adapter, page);
    this.pages.set(pageId, handle);
    return handle;
  }

  async closePage(pageId: string): Promise<void> {
    const handle = this.pages.get(pageId);
    if (!handle) return;
    this.pages.delete(pageId);
    await handle.page.close().catch(() => undefined);
  }

  getPage(pageId: string): PageHandle | undefined {
    return this.pages.get(pageId);
  }

  async shutdown(): Promise<void> {
    for (const handle of this.pages.values()) {
      await handle.page.close().catch(() => undefined);
    }
    this.pages.clear();
    if (this.context) {
      await this.context.close().catch(() => undefined);
      this.context = null;
    }
  }
  /**
   * Returns the least-busy tab for a provider, opening pool members if
   * they don't exist yet. Tab count stays fixed regardless of claim count.
   */
  async pickFromPool(
    provider: Provider,
    adapter: LlmAdapter,
    poolSize: number,
    pageLoadTimeoutMs: number,
  ): Promise<PageHandle> {
    const prefix = `${provider}-pool-`;

    for (let i = 0; i < poolSize; i++) {
      const pageId = `${prefix}${i}`;
      if (!this.pages.has(pageId)) {
        await this.newPage(provider, adapter, pageLoadTimeoutMs, pageId);
      }
    }

    const pool = [...this.pages.values()].filter((h) =>
      h.pageId.startsWith(prefix),
    );
    pool.sort((a, b) => a.pendingCount() - b.pendingCount());
    return pool[0];
  }

}
