import type { Page } from "playwright";

export type Provider = "claude" | "gemini" | "deepseek";

export type Action = "send_prompt" | "new_page" | "close_page" | "health_check" | "diagnose_page" | "clear_chats";

export interface IpcRequest {
  req_id: string;
  provider: Provider;
  action: Action;
  prompt: string;
  page_id: string;
}

export interface IpcResponseOk {
  req_id: string;
  status: "ok";
  text: string;
}

export interface IpcResponseError {
  req_id: string;
  status: "error";
  message: string;
}

export type IpcResponse = IpcResponseOk | IpcResponseError;

export interface LlmAdapter {
  readonly provider: Provider;
  readonly url: string;
  sendPrompt(page: Page, prompt: string): Promise<string>;
  isReady(page: Page): Promise<boolean>;
}
