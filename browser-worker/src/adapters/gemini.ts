import { BaseAdapter, type AdapterSelectors } from "./base.js";
import type { Provider } from "./types.js";

/**
 * Gemini web (gemini.google.com).
 *
 * STABILITY: Low — Google updates this UI most frequently of the three.
 * Expect to touch the stop-button and response selectors first.
 */
export class GeminiAdapter extends BaseAdapter {
  readonly provider: Provider = "gemini";
  readonly url = "https://gemini.google.com/app";

  protected getSelectors(): AdapterSelectors {
    return {
      input: [
        ".ql-editor",
        'rich-textarea [contenteditable="true"]',
        '[aria-label*="message"][contenteditable]',
        'div[role="textbox"][contenteditable]',
        '[contenteditable="true"]',
      ],
      sendButton: [
        'button[aria-label="Send message"]',
        ".send-button",
        "button.send-button",
      ],
      stopButton: [
        'button[aria-label="Stop generating"]',
        'button[aria-label="Stop"]',
      ],
      response: [
        ".response-content",
        "model-response",
        ".model-response-text",
        "message-content",
      ],
    };
  }
}
