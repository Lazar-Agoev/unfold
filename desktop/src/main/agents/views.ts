/**
 * What the window shows: it reports its board, page, selection, what the code panel
 * shows, the items in view, and the boards whose sessions it holds (`report_view`) when any of them
 * changes and when it gains focus. The main process keeps the one window's last report, so tools
 * default to its view and edits know whether it holds a board; the report goes when the window closes.
 */

import type { AgentBoardRef, CodePanelReport, CodePanelState, ViewReport, ViewSelection } from "../../shared/agent-api";

/** The most ids a report's lists may hold (far more boards or items in view than a window has). */
const MAX_IDS = 5_000;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

function boardRef(value: unknown): AgentBoardRef | null {
  return isRecord(value) && isString(value["project"]) && isString(value["id"]) ? { project: value["project"], id: value["id"] } : null;
}

function selection(value: unknown): ViewSelection | null {
  if (!isRecord(value)) {
    return null;
  }
  const { kind, id, card, key, owner, from, to, calls } = value;
  switch (kind) {
    case "none":
      return { kind };
    case "card":
    case "combined":
    case "link":
    case "frame":
    case "resource":
    case "note":
    case "proposal":
      return isString(id) ? { kind, id } : null;
    case "step":
      if (owner !== undefined && !isString(owner)) {
        return null;
      }
      return isString(card) && isString(key) ? { kind, card, key, ...(owner !== undefined ? { owner } : {}) } : null;
    case "arrow": {
      const keys = ids(calls);
      return isString(from) && isString(to) && keys !== null && keys.length > 0 ? { kind, from, to, calls: keys } : null;
    }
    default:
      return null;
  }
}

function ids(value: unknown): string[] | null {
  return Array.isArray(value) && value.length <= MAX_IDS && value.every(isString) ? value : null;
}

const CODE_STATES: ReadonlySet<unknown> = new Set<CodePanelState>(["current", "changed", "doesNotParse", "notAnalyzed", "notFound"]);

function isCodeState(value: unknown): value is CodePanelState {
  return CODE_STATES.has(value);
}

function isLine(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 1;
}

/** Lines `[from, to]`, one-based, or null when malformed. */
function lineRange(value: unknown): [number, number] | null {
  if (!Array.isArray(value) || value.length !== 2) {
    return null;
  }
  const from: unknown = value[0];
  const to: unknown = value[1];
  return isLine(from) && isLine(to) ? [from, to] : null;
}

/** What the code panel shows, or null when malformed. */
function codePanel(value: unknown): CodePanelReport | null {
  if (!isRecord(value)) {
    return null;
  }
  const { key, span, file, lines, widened, state } = value;
  const spanRange = span === undefined ? undefined : lineRange(span);
  const shown = lines === undefined ? undefined : lineRange(lines);
  if ((key !== undefined && !isString(key)) || (file !== undefined && !isString(file)) || spanRange === null || shown === null || typeof widened !== "boolean") {
    return null;
  }
  if (state !== undefined && !isCodeState(state)) {
    return null;
  }
  return {
    ...(key !== undefined ? { key } : {}),
    ...(spanRange !== undefined ? { span: spanRange } : {}),
    ...(file !== undefined ? { file } : {}),
    ...(shown !== undefined ? { lines: shown } : {}),
    widened,
    ...(state !== undefined ? { state } : {}),
  };
}

/** A window's `report_view` args as a report, or null when malformed. */
export function parseViewReport(value: unknown): ViewReport | null {
  if (!isRecord(value)) {
    return null;
  }
  const board = value["board"] === undefined ? undefined : boardRef(value["board"]);
  const page = value["page"];
  const chosen = selection(value["selection"]);
  const code = value["code"] === undefined ? undefined : codePanel(value["code"]);
  const visible = ids(value["visible"]);
  const held = Array.isArray(value["held"]) && value["held"].length <= MAX_IDS ? value["held"].map(boardRef) : null;
  if (board === null || (page !== undefined && !isString(page)) || !chosen || code === null || !visible || !held || held.some((ref) => ref === null)) {
    return null;
  }
  return {
    ...(board ? { board } : {}),
    ...(page !== undefined ? { page } : {}),
    selection: chosen,
    ...(code ? { code } : {}),
    visible,
    held: held as AgentBoardRef[],
  };
}

function sameBoard(a: AgentBoardRef, b: AgentBoardRef): boolean {
  return a.project === b.project && a.id === b.id;
}

const NO_REPORT: ViewReport = { selection: { kind: "none" }, visible: [], held: [] };

/** The window's last view, and the boards whose sessions it holds. */
export class ViewTracker {
  private last: ViewReport | undefined;

  /** The window reported its view. */
  report(report: ViewReport): void {
    this.last = report;
  }

  /** The window closed: nothing is in view, and it holds no board. */
  clear(): void {
    this.last = undefined;
  }

  /** The window's last report; undefined before it reports, and after it closes. */
  current(): ViewReport | undefined {
    return this.last;
  }

  /** Whether the window holds the board's session, as it last reported or as the bridge loaded it since. */
  holds(board: AgentBoardRef): boolean {
    return this.last?.held.some((ref) => sameBoard(ref, board)) === true;
  }

  /** The window now holds a board's session (loaded for an agent), before its next report says so. */
  hold(board: AgentBoardRef): void {
    const report = this.last ?? NO_REPORT;
    if (!report.held.some((ref) => sameBoard(ref, board))) {
      this.last = { ...report, held: [...report.held, board] };
    }
  }

  /** The window turned out not to hold a board its last report listed. */
  release(board: AgentBoardRef): void {
    if (this.last) {
      this.last = { ...this.last, held: this.last.held.filter((ref) => !sameBoard(ref, board)) };
    }
  }
}
