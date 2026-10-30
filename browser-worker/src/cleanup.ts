// Chat cleanup. Real selectors captured from live DOM dumps.
//
// Logs every step to diagnostics/<provider>-cleanup-<timestamp>.log
// so failures are readable after the fact.

import { writeFileSync, appendFileSync, mkdirSync } from "node:fs";
import type { Page } from "playwright";

const STEP_TIMEOUT_MS = 8_000;
const SETTLE_MS = 500;

let LOG_PATH = "";
function log(line: string): void {
  console.error(line);
  if (LOG_PATH) {
    try {
      appendFileSync(LOG_PATH, line + "\n");
    } catch {
      /* ignore */
    }
  }
}

function openLog(provider: string): void {
  mkdirSync("diagnostics", { recursive: true });
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  LOG_PATH = `diagnostics/${provider}-cleanup-${stamp}.log`;
  writeFileSync(LOG_PATH, `=== ${provider} cleanup @ ${new Date().toISOString()} ===\n`);
}

async function step<T>(label: string, fn: () => Promise<T>): Promise<T> {
  log(`[cleanup] → ${label}`);
  try {
    const r = await fn();
    log(`[cleanup] ✓ ${label}`);
    return r;
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    log(`[cleanup] ✗ ${label}: ${msg}`);
    throw new Error(`${label}: ${msg}`);
  }
}

/* ================================================================== */
/* Claude — bulk delete via ⋮ → Select → Select all                    */
/* ================================================================== */
export async function clearClaudeChats(page: Page): Promise<number> {
  openLog("claude");
  await step("goto claude.ai", () =>
    page.goto("https://claude.ai/new", { waitUntil: "domcontentloaded", timeout: 30_000 }),
  );
  await page.waitForTimeout(4_000);

  const before = await page.locator('button[aria-label^="More options for"]').count();
  log(`[cleanup] claude: ${before} chats before`);
  if (before === 0) return 0;

  // Step 1: open any chat's ⋮ menu.
  await step("open first chat ⋮ menu", async () => {
    const more = page.locator('button[aria-label^="More options for"]').first();
    await more.click({ timeout: STEP_TIMEOUT_MS, force: true });
  });
  await page.waitForTimeout(1_000);

  // Step 2: click "Select" in the popup.
  await step("click Select", async () => {
    await page
      .locator('text="Select", [role="menuitem"]:has-text("Select"), button:has-text("Select")')
      .first()
      .click({ timeout: 5_000, force: true });
  });
  await page.waitForTimeout(1_000);

  // Step 3: click "Select all".
  await step("click Select all", async () => {
    await page
      .locator('text="Select all", button:has-text("Select all"), [role="button"]:has-text("Select all")')
      .first()
      .click({ timeout: 5_000, force: true });
  });
  await page.waitForTimeout(800);

  // Step 4: click Delete.
  await step("click Delete", async () => {
    await page
      .locator('button:has-text("Delete")')
      .first()
      .click({ timeout: 5_000, force: true });
  });
  await page.waitForTimeout(800);

  // Step 5: confirm.
  await step("confirm Delete", async () => {
    await page
      .locator('button:has-text("Delete"), button:has-text("Confirm")')
      .last()
      .click({ timeout: 5_000, force: true });
  });
  await page.waitForTimeout(3_000);

  log(`[cleanup] claude: bulk delete attempted (before=${before})`);
  return before;
}


/* ================================================================== */
/* Gemini — per-chat with reload between iterations                    */
/* ================================================================== */
export async function clearGeminiChats(page: Page): Promise<number> {
  openLog("gemini");
  await page.goto("https://gemini.google.com/app", {
    waitUntil: "domcontentloaded",
    timeout: 30_000,
  });
  await page.waitForTimeout(4_000);

  let deleted = 0;
  const MAX = 500;

  for (let i = 0; i < MAX; i++) {
    try {
      const more = page.locator('button[aria-label^="More options for"]').first();
      if ((await more.count()) === 0) {
        log(`[cleanup] gemini: no more chats after ${deleted}`);
        break;
      }

      const label = await more.getAttribute("aria-label");
      log(`[cleanup] gemini: ${i + 1}) ${label}`);

      // Step 1: ⋮ button (force-click)
      await more.click({ timeout: 4_000, force: true }).catch(async () => {
        await more.evaluate((el: HTMLElement) => el.click());
      });
      await page.waitForTimeout(700);

      // Step 2: intermediate "Open menu for conversation actions."
      const opener = page.locator('button[aria-label="Open menu for conversation actions."]').first();
      if ((await opener.count()) === 0) {
        log(`[cleanup] gemini: ${i + 1}) opener missing → reload and retry`);
        await page.reload({ waitUntil: "domcontentloaded", timeout: 20_000 });
        await page.waitForTimeout(3_000);
        continue;
      }
      await opener.click({ timeout: 4_000, force: true }).catch(async () => {
        await opener.evaluate((el: HTMLElement) => el.click());
      });
      await page.waitForTimeout(700);

      // Step 3: Delete menu item
      const del = page.locator('[role="menuitem"]:has-text("Delete")').first();
      if ((await del.count()) === 0) {
        log(`[cleanup] gemini: ${i + 1}) Delete item missing → reload and retry`);
        await page.reload({ waitUntil: "domcontentloaded", timeout: 20_000 });
        await page.waitForTimeout(3_000);
        continue;
      }
      await del.click({ timeout: 4_000, force: true }).catch(async () => {
        await del.evaluate((el: HTMLElement) => el.click());
      });
      await page.waitForTimeout(700);

      // Step 4: confirm dialog (best-effort)
      const confirm = page
        .locator('button:has-text("Delete"), button:has-text("Confirm")')
        .last();
      await confirm.click({ timeout: 3_000, force: true }).catch(() => {});
      await page.waitForTimeout(1_500);

      deleted++;

      // Reload to reset the sidebar (avoids the "opener missing" bug).
      await page.reload({ waitUntil: "domcontentloaded", timeout: 20_000 });
      await page.waitForTimeout(2_500);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      log(`[cleanup] gemini: iteration ${i} error — ${msg}`);
      await page.reload({ waitUntil: "domcontentloaded", timeout: 20_000 }).catch(() => {});
      await page.waitForTimeout(2_500);
      continue;
    }
  }

  log(`[cleanup] gemini: deleted ${deleted}`);
  return deleted;
}





/* ================================================================== */
/* DeepSeek — bulk delete via profile → Settings → Data → Delete all   */
/* ================================================================== */
export async function clearDeepSeekChats(page: Page): Promise<number> {
  openLog("deepseek");
  await page.goto("https://chat.deepseek.com/", {
    waitUntil: "domcontentloaded",
    timeout: 30_000,
  });
  await page.waitForTimeout(4_000);

  const before = await page.locator('a[href^="/a/chat/s/"]').count();
  log(`[cleanup] deepseek: ${before} chats before`);
  if (before === 0) return 0;

  const forceClick = async (loc: any, label: string): Promise<boolean> => {
    if ((await loc.count()) === 0) return false;
    try {
      await loc.click({ timeout: 4_000, force: true });
      log(`[cleanup] deepseek: clicked ${label}`);
      return true;
    } catch {
      try {
        await loc.evaluate((el: HTMLElement) => el.click());
        log(`[cleanup] deepseek: JS-clicked ${label}`);
        return true;
      } catch (e) {
        log(`[cleanup] deepseek: failed ${label} — ${(e as Error).message}`);
        return false;
      }
    }
  };

  const trySelectors = async (sels: string[], label: string): Promise<boolean> => {
    for (const sel of sels) {
      const loc = page.locator(sel).last();
      if (await forceClick(loc, `${label} (${sel})`)) return true;
    }
    log(`[cleanup] deepseek: none matched for ${label}`);
    return false;
  };

  // Open profile (bottom-left of the sidebar). Matched by structure, not by
  // the account's display name, so no personal info lives in the code.
  if (!(await trySelectors([
    'div[class*="avatar"] >> nth=-1',
    'img[class*="avatar"] >> nth=-1',
    'div[class*="user"] >> nth=-1',
  ], "profile"))) return 0;
  await page.waitForTimeout(1_000);

  // Settings
  if (!(await trySelectors([
    '[role="menuitem"]:has-text("Settings")',
    'button:has-text("Settings")',
    'text="Settings" >> nth=-1',
  ], "settings"))) return 0;
  await page.waitForTimeout(1_500);

  // Data tab
  if (!(await trySelectors([
    '[role="tab"]:has-text("Data")',
    'button:has-text("Data")',
    'text="Data" >> nth=0',
  ], "data tab"))) return 0;
  await page.waitForTimeout(1_000);

  // Dump every visible button/link/div-button in the settings modal.
  const modalSnapshot = await page.evaluate(() => {
    const out: { tag: string; role: string; text: string; aria: string }[] = [];
    const seen = new Set<Element>();
    for (const sel of ["button", '[role="button"]', "a", "div[class*=button]"]) {
      document.querySelectorAll(sel).forEach((el) => {
        if (seen.has(el)) return;
        seen.add(el);
        const h = el as HTMLElement;
        const r = h.getBoundingClientRect();
        if (r.width === 0 || r.height === 0) return;
        const text = (h.innerText ?? "").trim();
        if (!text) return;
        out.push({
          tag: el.tagName.toLowerCase(),
          role: el.getAttribute("role") ?? "",
          text: text.slice(0, 80),
          aria: h.getAttribute("aria-label") ?? "",
        });
      });
    }
    return out;
  });
  log(`[cleanup] deepseek: visible clickables after Data tab (${modalSnapshot.length}):`);
  for (const b of modalSnapshot) {
    log(`[cleanup] deepseek:   <${b.tag} role="${b.role}" aria="${b.aria}">${b.text}`);
  }

  // Wait longer — Data tab content may load async
  await page.waitForTimeout(2_000);

  // Try every plausible selector for the delete button
  let deleted = false;
  const deleteSels = [
    'button:has-text("Delete all")',
    'div[role="button"]:has-text("Delete all")',
    'a:has-text("Delete all")',
    'text="Delete all"',
    'text="Delete all chats"',
    'button:has-text("Delete")',
  ];
  for (const sel of deleteSels) {
    const loc = page.locator(sel).last();
    if ((await loc.count()) === 0) continue;
    try {
      await loc.click({ timeout: 3_000, force: true });
      log(`[cleanup] deepseek: delete-all clicked via ${sel}`);
      deleted = true;
      break;
    } catch {
      continue;
    }
  }
  if (!deleted) {
    log(`[cleanup] deepseek: no delete-all button found — see clickables dump above`);
    return 0;
  }

  await page.waitForTimeout(1_500);

  // DUMP everything visible now — dialog should be up
  const snapshot = await page.evaluate(() => {
    const out: { tag: string; role: string; text: string; aria: string; cls: string }[] = [];
    const seen = new Set<Element>();
    for (const sel of [
      "button", '[role="button"]', '[role="menuitem"]',
      '[role="dialog"] button', "input[type=text]", "textarea",
    ]) {
      document.querySelectorAll(sel).forEach((el) => {
        if (seen.has(el)) return;
        seen.add(el);
        const h = el as HTMLElement;
        const r = h.getBoundingClientRect();
        if (r.width === 0 || r.height === 0) return;
        out.push({
          tag: el.tagName.toLowerCase(),
          role: el.getAttribute("role") ?? "",
          text: (h.innerText ?? "").trim().slice(0, 60),
          aria: h.getAttribute("aria-label") ?? "",
          cls: (h.className ?? "").toString().slice(0, 60),
        });
      });
    }
    return out;
  });
  log(`[cleanup] deepseek: buttons after Delete-all-chats click (${snapshot.length}):`);
  for (const b of snapshot) {
    log(`[cleanup] deepseek:   <${b.tag} role="${b.role}" aria="${b.aria}" cls="${b.cls}">${b.text}`);
  }

  // Try every plausible confirm selector
  const confirmSels = [
    // Exact match from real DOM dump — div[role=button] with error styling
    'div[role="button"].ds-button--error:has-text("Delete all chats")',
    'div[role="button"].ds-button--error:has-text("Delete all")',
    'div[role="button"].ds-button--error',
    // Generic fallbacks
    'div[role="button"]:has-text("Delete all chats")',
    'button:has-text("Delete all chats")',
    'button:has-text("Delete all")',
    'button:has-text("Confirm")',
  ];
  let confirmed = false;
  for (const sel of confirmSels) {
    const loc = page.locator(sel).last();
    if ((await loc.count()) === 0) continue;
    try {
      await loc.click({ timeout: 3_000, force: true });
      log(`[cleanup] deepseek: confirmed via ${sel}`);
      confirmed = true;
      break;
    } catch {
      continue;
    }
  }
  if (!confirmed) {
    log(`[cleanup] deepseek: no confirm selector matched — see buttons dump above`);
  }
  await page.waitForTimeout(4_000);

  // Wait for the deletion to actually complete, then re-count.
  await page.waitForTimeout(5_000);
  const after = await page.locator('a[href^="/a/chat/s/"]').count();
  const actuallyDeleted = Math.max(0, before - after);
  log(`[cleanup] deepseek: before=${before} after=${after} deleted=${actuallyDeleted}`);
  return actuallyDeleted;
}





/* ================================================================== */
/* Diagnostics                                                         */
/* ================================================================== */
export interface Diagnosis {
  provider: string;
  url: string;
  screenshotPath: string;
  buttons: { text: string; ariaLabel: string; visible: boolean }[];
  headings: string[];
  linksSample: { href: string; text: string }[];
}

export async function diagnoseProvider(
  page: Page,
  provider: string,
  url: string,
): Promise<Diagnosis> {
  console.error(`[diagnose] ${provider}: navigating to ${url}`);
  await page.goto(url, { waitUntil: "domcontentloaded", timeout: 30_000 });
  await page.waitForTimeout(4_000);

  mkdirSync("diagnostics", { recursive: true });
  const screenshotPath = `diagnostics/${provider}-${Date.now()}.png`;
  await page.screenshot({ path: screenshotPath, fullPage: false });
  console.error(`[diagnose] ${provider}: screenshot at ${screenshotPath}`);

  const buttons = await page.evaluate(() => {
    const out: { text: string; ariaLabel: string; visible: boolean }[] = [];
    document.querySelectorAll("button, [role=button]").forEach((el) => {
      const h = el as HTMLElement;
      const r = h.getBoundingClientRect();
      const visible = r.width > 0 && r.height > 0 && h.offsetParent !== null;
      out.push({
        text: (h.innerText ?? "").trim().slice(0, 80),
        ariaLabel: h.getAttribute("aria-label") ?? "",
        visible,
      });
    });
    return out;
  });

  const headings = await page.evaluate(() => {
    const out: string[] = [];
    document.querySelectorAll("h1, h2, h3, [role=heading]").forEach((el) => {
      const t = (el as HTMLElement).innerText?.trim();
      if (t) out.push(t.slice(0, 120));
    });
    return out;
  });

  const linksSample = await page.evaluate(() => {
    const out: { href: string; text: string }[] = [];
    document.querySelectorAll("a").forEach((el) => {
      const a = el as HTMLAnchorElement;
      const r = a.getBoundingClientRect();
      if (r.width > 0 && r.height > 0) {
        out.push({
          href: a.getAttribute("href") ?? "",
          text: (a.innerText ?? "").trim().slice(0, 80),
        });
      }
    });
    return out.slice(0, 60);
  });

  const diag: Diagnosis = { provider, url, screenshotPath, buttons, headings, linksSample };
  writeFileSync(
    `diagnostics/${provider}-${Date.now()}.json`,
    JSON.stringify(diag, null, 2),
  );
  console.error(
    `[diagnose] ${provider}: ${buttons.length} buttons, ${headings.length} headings, ${linksSample.length} links`,
  );
  return diag;
}
