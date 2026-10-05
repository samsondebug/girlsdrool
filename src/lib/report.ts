/**
 * The one place an unexpected error is surfaced. Nothing is swallowed: the message reaches the
 * person through the notice store, and the core's log never receives webview text (ADR-0015).
 */
import { toAppError } from "./ipc";
import { useUiStore } from "./store";

export function reportError(error: unknown, context: string): void {
  const appError = toAppError(error);
  useUiStore.getState().pushNotice({
    tone: "negative",
    text: `${context}: ${appError.message}`,
  });
}
