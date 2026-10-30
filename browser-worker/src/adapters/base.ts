import type { Locator, Page } from "playwright";
import type { LlmAdapter, Provider } from "./types.js";

/**
 * Substrings that indicate the LLM is still producing its answer. Claude
 * and Gemini render thought-summary headers ("Verifying a factual claim
 * ...") and status badges ("running") that look stable while the real
 * JSON answer has not arrived yet. When any of these appear in the
 * captured text we refuse to consider the response complete.
 *
 * Short markers are matched with word boundaries to avoid false positives
 * (e.g. "running" inside a URL or filename).
 */
const IN_PROGRESS_MARKERS: string[] = [
  "running",
  "Thinking",
  "Verifying a factual claim",
  "Searching the web",
  "Analyzing",
];

function containsInProgressMarker(text: string): boolean {
  for (const m of IN_PROGRESS_MARKERS) {
    if (m === "running" || m === "Thinking") {
      const re = new RegExp(`\\b${m}\\b`, "i");
      if (re.test(text)) return true;
    } else if (text.includes(m)) {
      return true;
    }
  }
  return false;
}

export interface AdapterSelectors {
  input: string[];
  sendButton: string[];
  stopButton: string[];
  response: string[];
}

export abstract class BaseAdapter implements LlmAdapter {
  abstract readonly provider: Provider;
  abstract readonly url: string;

  constructor(
    protected readonly responseWaitMs: number,
    protected readonly retries: number,
  ) {}

  protected abstract getSelectors(): AdapterSelectors;

  async isReady(page: Page): Promise<boolean> {
    const { input } = this.getSelectors();
    for (const sel of input) {
      if (await page.locator(sel).first().isVisible().catch(() => false)) {
        return true;
      }
    }
    return false;
  }


  async sendPrompt(page: Page, prompt: string): Promise<string> {
    let lastError: Error | null = null;

    for (let attempt = 0; attempt <= this.retries; attempt++) {
      if (attempt > 0) {
        console.error(`[${this.provider}] retry ${attempt}/${this.retries} after: ${lastError?.message}`);
      }
      try {
        return await this.attempt(page, prompt);
      } catch (e) {
        lastError = e as Error;
        const msg = lastError.message ?? "";
        if (/detached|navigation|Target closed|Execution context was destroyed|frame was detached|Cannot find context/i.test(msg)) {
          console.error(`[${this.provider}] reloading page after navigation error`);
          await page.reload({ waitUntil: "domcontentloaded", timeout: 20_000 }).catch(() => undefined);
          await page.waitForTimeout(1_500);
        }
      }
    }

    throw lastError ?? new Error(`${this.provider}: sendPrompt failed`);
  }

  private async attempt(page: Page, prompt: string): Promise<string> {const sel = this.getSelectors();

    const input = await findFirstVisible(page, sel.input, 10_000);
    const beforeText = (await this.lastResponseText(page, sel.response)).trim();

    await this.fillInput(page, input, prompt);
    await this.clickSend(page, sel.sendButton);
    await this.waitForNewResponse(page, sel.response, beforeText);
    await this.waitForStreamingStop(page, sel.stopButton);

    const text = (await this.lastResponseText(page, sel.response)).trim();
    if (!text) {
      throw new Error(`${this.provider}: extracted response was empty`);
    }
    return text;
  }

  private async fillInput(page: Page, input: Locator, text: string): Promise<void> {
    const tag = await input.evaluate((el) => el.tagName.toLowerCase());

    if (tag === "textarea" || tag === "input") {
      await input.fill(text);
      return;
    }

    await input.click();
    await page.waitForTimeout(150);
    await page.keyboard.press("Control+A");
    await page.keyboard.press("Backspace");
    await page.waitForTimeout(100);

    const inserted = await input
      .evaluate((el, t) => {
        el.focus();
        while (el.firstChild) el.removeChild(el.firstChild);
        try {
          // eslint-disable-next-line deprecation/deprecation
          const ok = document.execCommand("insertText", false, t);
          return ok && (el.textContent ?? "").length >= Math.min(8, t.length);
        } catch {
          return false;
        }
      }, text)
      .catch(() => false);

    if (!inserted) {
      await input.click();
      await page.keyboard.press("Control+A");
      await page.keyboard.press("Backspace");
      await page.keyboard.insertText(text);
      await page.waitForTimeout(250);
    }
  }

  private async clickSend(page: Page, selectors: string[]): Promise<void> {
    let sendBtn: Locator | null = null;
    try {
      sendBtn = await findFirstVisible(page, selectors, 5_000);
    } catch {
      sendBtn = null;
    }

    if (sendBtn) {
      const deadline = Date.now() + 3_000;
      while (Date.now() < deadline) {
        const disabled = await sendBtn.isDisabled().catch(() => true);
        if (!disabled) break;
        await page.waitForTimeout(150);
      }
      const stillDisabled = await sendBtn.isDisabled().catch(() => false);
      if (!stillDisabled) {
        await sendBtn.click();
        return;
      }
    }

    await page.keyboard.press("Enter");
  }

  private async waitForNewResponse(page: Page, responseSelectors: string[], beforeText: string): Promise<void> {
    const deadline = Date.now() + this.responseWaitMs;
    while (Date.now() < deadline) {
      const current = (await this.lastResponseText(page, responseSelectors)).trim();
      if (current && current !== beforeText) return;
      await page.waitForTimeout(300);
    }
    console.error(`[${this.provider}] no new response detected within timeout`);
  }

  private async waitForStreamingStop(page: Page, stopSelectors: string[]): Promise<void> {
    const deadline = Date.now() + this.responseWaitMs;
    const minWaitUntil = Date.now() + 2_000;

    // A response is "done" when its text hasn't changed for STABLE_POLLS
    // consecutive polls. This is the primary signal; the stop button is
    // only a hint. Claude's "thinking" phase renders no stop button for
    // several seconds, which is what broke the previous version.
    const POLL_MS = 400;
    const STABLE_POLLS = 5;      // 2.0s of text silence
    const MIN_GROWTH_BYTES = 20; // shorter than this is UI chrome, not an answer

    const responseSelectors = this.getSelectors().response;
    let lastText = "";
    let stableCount = 0;
    let sawAnyText = false;
    let sawStreamingIndicator = false;
    let stopButtonSeenAt: number | null = null;

    await page.waitForTimeout(700);

    while (Date.now() < deadline) {
      const isStreaming = await this.isAnyVisible(page, stopSelectors);
      if (isStreaming) {
        sawStreamingIndicator = true;
        stopButtonSeenAt = Date.now();
      }

      const currentText = await this.lastResponseText(page, responseSelectors);
      if (currentText.length > MIN_GROWTH_BYTES) sawAnyText = true;

      if (
        currentText === lastText &&
        currentText.length > 0 &&
        !containsInProgressMarker(currentText)
      ) {
        stableCount++;
      } else {
        stableCount = 0;
      }
      lastText = currentText;

      // Primary completion signal: text has stabilized and is substantial.
      if (
        sawAnyText &&
        stableCount >= STABLE_POLLS &&
        Date.now() > minWaitUntil &&
        (currentText.includes("{") || currentText.includes("["))
      ) {
        return;
      }

      // Secondary: we saw a stop button and it has now been gone for >1s
      // and text is non-trivial. Verify stability with one more poll.
      if (
        sawStreamingIndicator &&
        !isStreaming &&
        stopButtonSeenAt !== null &&
        Date.now() - stopButtonSeenAt > 1_000 &&
        sawAnyText
      ) {
        await page.waitForTimeout(POLL_MS);
        const after = await this.lastResponseText(page, responseSelectors);
        if (after === currentText) return;
      }

      await page.waitForTimeout(POLL_MS);
    }

    console.error(
      `[${this.provider}] waitForStreamingStop timed out after ${this.responseWaitMs}ms`,
    );
  }

  private async isAnyVisible(page: Page, selectors: string[]): Promise<boolean> {
    for (const sel of selectors) {
      if (await page.locator(sel).first().isVisible().catch(() => false)) {
        return true;
      }
    }
    return false;
  }

  private async lastResponseText(page: Page, selectors: string[]): Promise<string> {
    for (const sel of selectors) {
      const loc = page.locator(sel);
      const n = await loc.count().catch(() => 0);
      if (n === 0) continue;
      const text = await loc.nth(n - 1).innerText().catch(() => "");
      if (text.trim()) return text;
    }
    return "";
  }
}

async function findFirstVisible(page: Page, selectors: string[], timeoutMs: number): Promise<Locator> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    for (const sel of selectors) {
      const loc = page.locator(sel).first();
      if (await loc.isVisible().catch(() => false)) {
        return loc;
      }
    }
    await page.waitForTimeout(150);
  }
  throw new Error(`no visible element matched any of: ${selectors.join(", ")}`);
}
