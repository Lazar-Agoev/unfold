/**
 * The agent channel in this window: the main process's requests (`agent_edit`, `agent_read`,
 * `agent_open_board`, `agent_create_board`, `agent_edit_board`, `agent_show`, `agent_repair`,
 * `agent_finish`) and the answers (`agent_edit_result`, `agent_read_result`,
 * `agent_open_board_result`, `agent_create_board_result`, `agent_edit_board_result`,
 * `agent_show_result`, `agent_repair_result`). It is installed before the window renders, so a request
 * sent while the window loads isn't missed, and requests wait until the window's own state is restored
 * (restoring replaces the tabs). Working marks of boards the window no longer holds are dropped.
 */

import { onShellEvent } from "../../../ipc/unfold";
import { windowCommand } from "../engine";
import { useBoards } from "../store";
import { createBoardForAgent, editBoardForAgent, showForAgent } from "./boards";
import { applyAgentEdit, finishAgentWork, openBoardForAgent, readAgentPage } from "./edit";
import { repairForAgent } from "./repair";
import { pruneWorking } from "./working";

const waiting: (() => void)[] = [];
let ready = false;
let uninstall: (() => void) | null = null;

function whenReady(run: () => void): void {
  if (ready) {
    run();
  } else {
    waiting.push(run);
  }
}

/** Runs a request and sends its answer; an answer that can't be sent leaves the main process to time out. */
function answering(send: () => Promise<unknown>): void {
  whenReady(() => {
    send().catch((error: unknown) => {
      console.warn("Answering an agent's request failed:", error);
    });
  });
}

/** Listens for agents' requests (idempotent). Install it before the UI renders. */
export function installAgentChannel(): void {
  if (uninstall) {
    return;
  }
  const stops = [
    onShellEvent("agent_edit", (request) => {
      answering(async () => windowCommand("agent_edit_result", { requestId: request.requestId, result: await applyAgentEdit(request) }));
    }),
    onShellEvent("agent_read", (request) => {
      answering(() => windowCommand("agent_read_result", { requestId: request.requestId, result: readAgentPage(request) }));
    }),
    onShellEvent("agent_open_board", (request) => {
      answering(async () => windowCommand("agent_open_board_result", { requestId: request.requestId, result: await openBoardForAgent(request) }));
    }),
    onShellEvent("agent_create_board", (request) => {
      answering(async () => windowCommand("agent_create_board_result", { requestId: request.requestId, result: await createBoardForAgent(request) }));
    }),
    onShellEvent("agent_edit_board", (request) => {
      answering(async () => windowCommand("agent_edit_board_result", { requestId: request.requestId, result: await editBoardForAgent(request) }));
    }),
    onShellEvent("agent_show", (request) => {
      answering(() => windowCommand("agent_show_result", { requestId: request.requestId, result: showForAgent(request) }));
    }),
    onShellEvent("agent_repair", (request) => {
      answering(async () => windowCommand("agent_repair_result", { requestId: request.requestId, result: await repairForAgent(request) }));
    }),
    onShellEvent("agent_finish", (finish) => {
      whenReady(() => {
        finishAgentWork(finish);
      });
    }),
    useBoards.subscribe((state, previous) => {
      if (state.boards !== previous.boards) {
        pruneWorking(new Set(Object.keys(state.boards)));
      }
    }),
  ];
  uninstall = () => {
    for (const stop of stops) {
      stop();
    }
  };
}

/** The window's own state is restored: run the requests that arrived meanwhile, and later ones at once. */
export function setAgentChannelReady(): void {
  ready = true;
  for (const run of waiting.splice(0)) {
    run();
  }
}

/** Resets the channel (tests). */
export function resetAgentChannel(): void {
  uninstall?.();
  uninstall = null;
  waiting.length = 0;
  ready = false;
}
