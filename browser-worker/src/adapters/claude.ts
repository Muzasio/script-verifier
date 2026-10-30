import { BaseAdapter, type AdapterSelectors } from "./base.js";
import type { Provider } from "./types.js";

export class ClaudeAdapter extends BaseAdapter {
  readonly provider: Provider = "claude";
  readonly url = "https://claude.ai/new";

  protected getSelectors(): AdapterSelectors {
    return {
      input: [
        "div.ProseMirror",
        '[data-placeholder][contenteditable="true"]',
        '[data-testid="composer-input"]',
        '[aria-label="Message Claude"][contenteditable="true"]',
        'div[role="textbox"][contenteditable="true"]',
        '[contenteditable="true"]',
      ],
      sendButton: [
        'button[aria-label="Send Message"]',
        'button[aria-label="Send message"]',
        'button[data-testid="send-button"]',
        'button[type="submit"]',
      ],
      stopButton: [
        'button[aria-label="Stop"]',
        'button[aria-label="Stop generating"]',
        'button[data-testid="stop-button"]',
      ],
      response: [".font-claude-response", "[data-is-streaming]", ".font-claude-message"],
    };
  }
}
