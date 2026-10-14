/**
 * The Show all calls switch in the board's top bar: whether cards draw builtin calls too. It is one
 * switch for the window, not part of any board. The main process saves it with the window: it comes
 * back from `window_info` at launch (`restoreShowAllCalls`), and every change is reported
 * (`report_show_all_calls`).
 */

import { create } from "zustand";
import { windowCommand, type WindowInfo } from "../ipc/unfold";

interface ShowAllCallsState {
  readonly on: boolean;
}

const useShowAllCallsStore = create<ShowAllCallsState>()(() => ({ on: false }));

/** Whether Show all calls is on, kept current. */
export function useShowAllCalls(): boolean {
  return useShowAllCallsStore((state) => state.on);
}

/** Whether Show all calls is on now, outside React. */
export function getShowAllCalls(): boolean {
  return useShowAllCallsStore.getState().on;
}

export function setShowAllCalls(on: boolean): void {
  useShowAllCallsStore.setState({ on });
}

/** Puts the switch back as `window_info` says the window had it (off outside the app). */
export function restoreShowAllCalls(info: WindowInfo | null): void {
  setShowAllCalls(info?.showAllCalls ?? false);
}

async function report(on: boolean): Promise<void> {
  try {
    await windowCommand("report_show_all_calls", { on });
  } catch (error) {
    console.warn("Reporting Show all calls failed:", error);
  }
}

/** Tells the main process whenever the switch changes. Returns the stop function. */
export function reportShowAllCalls(): () => void {
  return useShowAllCallsStore.subscribe((state, previous) => {
    if (state.on !== previous.on) {
      void report(state.on);
    }
  });
}
