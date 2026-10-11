/** Waiting for a drawn page to settle: every card measured, its data loaded, and nothing moving. */

/** How often the page is looked at. */
const POLL_MS = 40;

export interface QuietOptions {
  /** How long the signature must stay the same. */
  readonly quietMs: number;
  /** How long to wait at most. */
  readonly timeoutMs: number;
  readonly signal: AbortSignal;
}

/**
 * Resolves true once `probe` has returned the same signature, never null, for `quietMs`; false after
 * `timeoutMs`, or when `signal` aborts. `probe` returns null while the page isn't ready at all.
 */
export function whenQuiet(probe: () => string | null, { quietMs, timeoutMs, signal }: QuietOptions): Promise<boolean> {
  return new Promise((resolve) => {
    const started = Date.now();
    let last: string | null = null;
    let since = started;
    const tick = () => {
      if (signal.aborted) {
        resolve(false);
        return;
      }
      const now = Date.now();
      const current = probe();
      if (current === null || current !== last) {
        last = current;
        since = now;
      } else if (now - since >= quietMs) {
        resolve(true);
        return;
      }
      if (now - started >= timeoutMs) {
        resolve(false);
        return;
      }
      setTimeout(tick, POLL_MS);
    };
    tick();
  });
}
