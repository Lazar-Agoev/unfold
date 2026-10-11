/**
 * What each window shows (design §5.6): windows report their board, page, selection, lens, the items
 * in view, and their board tabs (`report_view`) when any of them changes and when they gain focus. The
 * main process keeps the last report of each window and the order windows were used in, so tools
 * default to the view of the window used last and edits find the window that holds a board.
 */

import type { AgentBoardRef, AgentView, ViewReport, ViewSelection } from "../../shared/agent-api";

const VIEWS: ReadonlySet<string> = new Set<AgentView>(["diagram", "flow", "code"]);
/** The most ids a report's lists may hold (a window holds at most 100 tabs; a page, far fewer items in view). */
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

/** One call of a `calls` selection: its occurrence, its edge, and its line. */
function call(value: unknown): { source: string; seq: number; line: number | null } | null {
  if (!isRecord(value)) {
    return null;
  }
  const { source, seq, line } = value;
  return isString(source) && Number.isInteger(seq) && (line === null || Number.isInteger(line)) ? { source, seq: seq as number, line: line as number | null } : null;
}

function selection(value: unknown): ViewSelection | null {
  if (!isRecord(value)) {
    return null;
  }
  const { kind, id, card, key, from, to, project, source, seq, line, calls } = value;
  switch (kind) {
    case "none":
      return { kind };
    case "card":
    case "group":
    case "link":
    case "frame":
      return isString(id) ? { kind, id } : null;
    case "step":
      return isString(card) && isString(key) ? { kind, card, key } : null;
    case "line":
      return isString(from) && isString(to) && isString(project) && isString(source) && Number.isInteger(seq) && (line === null || Number.isInteger(line))
        ? { kind, from, to, project, source, seq: seq as number, line: line as number | null }
        : null;
    case "calls": {
      const listed = Array.isArray(calls) && calls.length <= MAX_IDS ? calls.map(call) : [null];
      const valid = listed.filter((item) => item !== null);
      return isString(from) && isString(to) && isString(project) && valid.length === listed.length ? { kind, from, to, project, calls: valid } : null;
    }
    default:
      return null;
  }
}

function ids(value: unknown): string[] | null {
  return Array.isArray(value) && value.length <= MAX_IDS && value.every(isString) ? value : null;
}

/** A window's `report_view` args as a report, or null when malformed. */
export function parseViewReport(value: unknown): ViewReport | null {
  if (!isRecord(value)) {
    return null;
  }
  const board = value["board"] === undefined ? undefined : boardRef(value["board"]);
  const page = value["page"];
  const lens = value["lens"];
  const chosen = selection(value["selection"]);
  const visible = ids(value["visible"]);
  const tabs = Array.isArray(value["tabs"]) && value["tabs"].length <= MAX_IDS ? value["tabs"].map(boardRef) : null;
  if (
    board === null ||
    (page !== undefined && !isString(page)) ||
    (lens !== undefined && !(typeof lens === "string" && VIEWS.has(lens))) ||
    !chosen ||
    !visible ||
    !tabs ||
    tabs.some((tab) => tab === null)
  ) {
    return null;
  }
  return {
    ...(board ? { board } : {}),
    ...(page !== undefined ? { page } : {}),
    ...(lens !== undefined ? { lens: lens as AgentView } : {}),
    selection: chosen,
    visible,
    tabs: tabs as AgentBoardRef[],
  };
}

function sameBoard(a: AgentBoardRef, b: AgentBoardRef): boolean {
  return a.project === b.project && a.id === b.id;
}

const NO_REPORT: ViewReport = { selection: { kind: "none" }, visible: [], tabs: [] };

/** The last view of each window, and which window was used last. */
export class ViewTracker {
  private readonly reports = new Map<number, ViewReport>();
  /** Window ids, least recently used first. */
  private used: number[] = [];

  /** A window reported its view. */
  report(windowId: number, report: ViewReport): void {
    this.reports.set(windowId, report);
  }

  /** A window gained focus: it is the one used last. */
  focused(windowId: number): void {
    this.used = [...this.used.filter((id) => id !== windowId), windowId];
  }

  /** A window closed. */
  forget(windowId: number): void {
    this.reports.delete(windowId);
    this.used = this.used.filter((id) => id !== windowId);
  }

  /** The window's last report. */
  get(windowId: number): ViewReport | undefined {
    return this.reports.get(windowId);
  }

  /** The window used last (by focus), or else the first that reported; undefined before any did either. */
  lastUsed(): number | undefined {
    return this.used.at(-1) ?? this.reports.keys().next().value;
  }

  /** The window whose tabs hold the board, if any reported it. */
  holder(board: AgentBoardRef): number | undefined {
    for (const [windowId, report] of this.reports) {
      if (report.tabs.some((tab) => sameBoard(tab, board))) {
        return windowId;
      }
    }
    return undefined;
  }

  /** A window now holds a board (opened for an agent), before its next report says so. */
  holds(windowId: number, board: AgentBoardRef): void {
    for (const [id, report] of this.reports) {
      if (id !== windowId && report.tabs.some((tab) => sameBoard(tab, board))) {
        this.reports.set(id, { ...report, tabs: report.tabs.filter((tab) => !sameBoard(tab, board)) });
      }
    }
    const report = this.reports.get(windowId) ?? NO_REPORT;
    if (!report.tabs.some((tab) => sameBoard(tab, board))) {
      this.reports.set(windowId, { ...report, tabs: [...report.tabs, board] });
    }
  }

  /** A window turned out not to hold a board its last report listed. */
  released(windowId: number, board: AgentBoardRef): void {
    const report = this.reports.get(windowId);
    if (report) {
      this.reports.set(windowId, { ...report, tabs: report.tabs.filter((tab) => !sameBoard(tab, board)) });
    }
  }
}
