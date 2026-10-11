// Send-side IPC helpers (MOE-0030 revision 9): the backend answers a chat send immediately — the
// worker streams the answer separately — so a silent stall there is a bug, not a wait. These
// helpers turn such stalls into visible errors instead of a dead conversation.

import { invoke } from "@tauri-apps/api/core";

/** The backend's answer window for a chat send; past it the failure is surfaced, not waited on. */
export const SEND_TIMEOUT_MS = 20_000;

/** `invoke` with a deadline: a hang rejects with a named error instead of freezing the surface. */
export function invokeWithTimeout<T>(
  command: string,
  args: Record<string, unknown> | undefined,
  what: string,
  ms: number = SEND_TIMEOUT_MS,
): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`${what} did not answer within ${Math.round(ms / 1000)}s`)),
      ms,
    );
    invoke<T>(command, args).then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (err) => {
        clearTimeout(timer);
        reject(err);
      },
    );
  });
}
