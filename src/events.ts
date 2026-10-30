import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Claim, ProgressEvent } from "./types";

export const PROGRESS_EVENT = "verification://progress";
export const CLAIM_UPDATE_EVENT = "verification://claim_update";

export function onProgress(handler: (payload: ProgressEvent) => void): Promise<UnlistenFn> {
  return listen<ProgressEvent>(PROGRESS_EVENT, (event) => handler(event.payload));
}

export function onClaimUpdate(handler: (payload: Claim) => void): Promise<UnlistenFn> {
  return listen<Claim>(CLAIM_UPDATE_EVENT, (event) => handler(event.payload));
}
