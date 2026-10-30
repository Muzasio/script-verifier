import { invoke } from "@tauri-apps/api/core";
import type { Claim, FinalResult, ProgressState } from "./types";

export function startVerification(scriptText: string): Promise<string> {
  return invoke<string>("start_verification", { scriptText });
}

export function stopVerification(scriptId: string): Promise<void> {
  return invoke<void>("stop_verification", { scriptId });
}

export function getProgress(scriptId: string): Promise<ProgressState> {
  return invoke<ProgressState>("get_progress", { scriptId });
}

export function getClaims(scriptId: string): Promise<Claim[]> {
  return invoke<Claim[]>("get_claims", { scriptId });
}

export function getResult(scriptId: string): Promise<FinalResult> {
  return invoke<FinalResult>("get_result", { scriptId });
}

/** Open a provider's page in the app's Chromium (for manual cleanup). */
export function openProviderPage(
  provider: "claude" | "gemini" | "deepseek",
): Promise<string> {
  return invoke<string>("open_provider_page", { provider });
}

/** Auto-delete all conversations on a provider. Returns count deleted. */
export function clearChats(
  provider: "claude" | "gemini" | "deepseek",
): Promise<number> {
  return invoke<number>("clear_chats", { provider });
}

/** Dump DOM structure of a provider's page (for debugging selectors). */
export function diagnosePage(
  provider: "claude" | "gemini" | "deepseek",
): Promise<string> {
  return invoke<string>("diagnose_page", { provider });
}

/** Write a diagnostic dump to disk. Returns the absolute path. */
export function writeDiagFile(provider: string, content: string): Promise<string> {
  return invoke<string>("write_diag_file", { provider, content });
}
