import { BaseAdapter, type AdapterSelectors } from "./base.js";
import type { Provider } from "./types.js";

export class DeepSeekAdapter extends BaseAdapter {
  readonly provider: Provider = "deepseek";
  readonly url = "https://chat.deepseek.com/";

  protected getSelectors(): AdapterSelectors {
    return {
      input: [
        "#chat-input",
        'textarea[placeholder*="Send a message"]',
        'textarea[placeholder*="Message DeepSeek"]',
        'textarea[data-testid="chat-input"]',
        'div[contenteditable][role="textbox"]',
        "textarea",
      ],
      sendButton: [
        'button[aria-label="Send message"]',
        '[data-testid="send-button"]',
        'button[type="submit"]',
      ],
      stopButton: [
        'button[aria-label="Stop generating"]',
        'button[aria-label="Stop"]',
        '[data-testid="stop-button"]',
      ],
      response: [".ds-markdown", '[data-message-author-role="assistant"]', '[class*="AssistantMessage"]', '[class*="markdown-body"]'],
    };
  }
}
