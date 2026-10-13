/**
 * What the window shows (design §5.6): it reports its board, page, selection, lens, the items in
 * view, and the boards whose sessions it holds (`report_view`) when any of them changes and when it
 * gains focus. The main process keeps the last report, so tools default to the window's view and
 * edits know whether it holds a board. Reports are kept by window id: each opening of the window gets
 * a new one.
 */

import type { AgentBoardRef, AgentLens, ViewReport, ViewSelection } from "../../shared/agent-api";

/** The most ids a report's lists may hold (far more boards or items in view than a window has). */
const MAX_IDS = 5_000;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

function isLens(value: unknown): value is AgentLens {
  return value === "diagram" || value === "code";
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
  const { kind, id, card, key, owner, from, to, source, seq, line, calls } = value;
  switch (kind) {
    case "none":
      return { kind };
    case "card":
    case "group":
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
    case "line":
      return isString(from) && isString(to) && isString(source) && Number.isInteger(seq) && (line === null || Number.isInteger(line))
        ? { kind, from, to, source, seq: seq as number, line: line as number | null }
        : null;
    case "calls": {
      const listed = Array.isArray(calls) && calls.length <= MAX_IDS ? calls.map(call) : [null];
      const valid = listed.filter((item) => item !== null);
      return isString(from) && isString(to) && valid.length === listed.length ? { kind, from, to, calls: valid } : null;
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
  const held = Array.isArray(value["held"]) && value["held"].length <= MAX_IDS ? value["held"].map(boardRef) : null;
  if (
    board === null ||
    (page !== undefined && !isString(page)) ||
    (lens !== undefined && !isLens(lens)) ||
    !chosen ||
    !visible ||
    !held ||
    held.some((ref) => ref === null)
  ) {
    return null;
  }
  return {
    ...(board ? { board } : {}),
    ...(page !== undefined ? { page } : {}),
    ...(isLens(lens) ? { lens } : {}),
    selection: chosen,
    visible,
    held: held as AgentBoardRef[],
  };
}

function sameBoard(a: AgentBoardRef, b: AgentBoardRef): boolean {
  return a.project === b.project && a.id === b.id;
}

const NO_REPORT: ViewReport = { selection: { kind: "none" }, visible: [], held: [] };

/** The last view of the window. */
export class ViewTracker {
  private readonly reports = new Map<number, ViewReport>();
  /** The window that reported last. */
  private last: number | undefined;

  /** A window reported its view. */
  report(windowId: number, report: ViewReport): void {
    this.reports.set(windowId, report);
    this.last = windowId;
  }

  /** A window closed. */
  forget(windowId: number): void {
    this.reports.delete(windowId);
    if (this.last === windowId) {
      this.last = undefined;
    }
  }

  /** The window's last report. */
  get(windowId: number): ViewReport | undefined {
    return this.reports.get(windowId);
  }

  /** The window that reported last, while it is open; undefined before it reports. */
  lastUsed(): number | undefined {
    return this.last;
  }

  /** The window that holds the board's session, if any reported it. */
  holder(board: AgentBoardRef): number | undefined {
    for (const [windowId, report] of this.reports) {
      if (report.held.some((ref) => sameBoard(ref, board))) {
        return windowId;
      }
    }
    return undefined;
  }

  /** A window now holds a board's session (loaded for an agent), before its next report says so. */
  holds(windowId: number, board: AgentBoardRef): void {
    for (const [id, report] of this.reports) {
      if (id !== windowId && report.held.some((ref) => sameBoard(ref, board))) {
        this.reports.set(id, { ...report, held: report.held.filter((ref) => !sameBoard(ref, board)) });
      }
    }
    const report = this.reports.get(windowId) ?? NO_REPORT;
    if (!report.held.some((ref) => sameBoard(ref, board))) {
      this.reports.set(windowId, { ...report, held: [...report.held, board] });
    }
  }

  /** A window turned out not to hold a board its last report listed. */
  released(windowId: number, board: AgentBoardRef): void {
    const report = this.reports.get(windowId);
    if (report) {
      this.reports.set(windowId, { ...report, held: report.held.filter((ref) => !sameBoard(ref, board)) });
    }
  }
}
