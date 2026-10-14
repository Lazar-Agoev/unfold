/**
 * `get_current_view` (design §5.6, §6.2): what the person is looking at in the window they used
 * last, from that window's view report, with the page as the window holds it (unsaved edits
 * included): the selection (a proposed step, or a step shown in a call opened in place with the
 * definition that owns it, among the rest), and the items in view. Loaded with
 * the session's first tools. It reads; it never moves the person's view.
 */

import { z } from "zod";
import type { ViewSelection } from "../../../shared/agent-api";
import type { Page } from "../engine-types";
import { cardKey, cardView } from "../page-code";
import { textResult, windowRefusal } from "../results";
import { defineTool } from "../tool";

/** The most items in view it lists; `read_page` reads the whole page. */
const MAX_LISTED = 60;

/** One item of the page, by id, as a line an agent can act on: its kind, id, and what it shows. */
function describeItem(page: Page, id: string): string {
  const card = page.cards.find((item) => item.id === id);
  if (card) {
    const calls = card.open?.length ?? 0;
    const facts = [cardView(page, card), calls > 0 ? `${calls} ${calls === 1 ? "call" : "calls"} open in place` : null];
    return `card ${id}: ${cardKey(card)} (${facts.filter(Boolean).join(", ")})`;
  }
  const proposal = page.proposals?.find((item) => item.id === id);
  if (proposal) {
    return `proposed step ${id}: "${proposal.title}", ${"after" in proposal ? `after ${proposal.after}` : `at the start of ${proposal.start}`} (an intention, not code)`;
  }
  const combined = page.combined.find((item) => item.id === id);
  if (combined) {
    return `combined step ${id}: "${combined.title}"${combined.collapsed ? " (collapsed)" : ""}`;
  }
  const link = page.links.find((item) => item.id === id);
  if (link) {
    const label = link.label ?? link.description;
    return `${link.kind} connection ${id}: ${link.from} → ${link.to}${label ? `, "${label.text}"` : ""}`;
  }
  const frame = page.frames?.find((item) => item.id === id);
  if (frame) {
    return `frame ${id}: "${frame.title}" (${frame.members.length} members)`;
  }
  const resource = page.resources?.find((item) => item.id === id);
  if (resource) {
    return `${resource.kind} ${id}: "${resource.name}"`;
  }
  const note = page.notes?.find((item) => item.id === id);
  return note ? `note ${id}: "${note.text}"` : `item ${id}`;
}

function selectionText(page: Page, selection: ViewSelection): string {
  switch (selection.kind) {
    case "none":
      return "Nothing is selected.";
    case "card":
    case "combined":
    case "link":
    case "frame":
    case "resource":
    case "note":
    case "proposal":
      return `Selected: ${describeItem(page, selection.id)}.`;
    case "step":
      return selection.owner === undefined
        ? `Selected: the step ${selection.key} of card ${selection.card}.`
        : `Selected: the step ${selection.key} of ${selection.owner}, shown in a call card ${selection.card} opens in place; read_code without key reads that definition.`;
    case "arrow":
      return `Selected: the arrow from card ${selection.from} to card ${selection.to}, which draws ${selection.calls.length === 1 ? "the call" : `${selection.calls.length} calls:`} ${selection.calls.join(", ")}. read_call with a call's key says what it sends.`;
  }
}

export const getCurrentView = defineTool({
  name: "get_current_view",
  title: "Get current view",
  description:
    "What the person is looking at in the Unfold window they used last: the project, board, and page (with its revision), the selection (a card, a step (one shown in a call opened in place names the definition that owns it), a combined step, a proposed step, a frame, a resource card, a note, a connection, or an arrow between two cards with the calls it draws), and the items in view, each with its id. Call it when the person says \"this\", \"here\", or \"the selected card\", and before working on the page they're viewing. Once you start, pass the board and page ids to other tools: the person may switch pages. It never moves their view.",
  input: z.object({}),
  annotations: { readOnlyHint: true },
  alwaysLoad: true,
  async run(_args, context) {
    const windowId = context.views.lastUsed();
    const report = windowId === undefined ? undefined : context.views.get(windowId);
    if (!report) {
      return textResult(
        context.header(),
        "No Unfold window has reported what it shows: Unfold is starting, or no window is open. Ask the person to open a board, or start from list_projects.",
      );
    }
    if (!report.board || report.page === undefined) {
      return textResult(
        context.header(),
        "The window shows Home, not a board.",
        "Next: ask the person which board to work on, or start from list_projects.",
      );
    }
    const read = await context.bridge.read({ board: report.board, page: report.page });
    if (!read.ok) {
      throw windowRefusal(read);
    }
    const page = read.page as unknown as Page;
    const project = (await context.projects()).find((row) => row.id === report.board?.project);
    const listed = report.visible.slice(0, MAX_LISTED);
    return textResult(
      context.header({
        project: project?.name ?? report.board.project,
        board: read.board.title,
        page: page.title,
        revision: read.revision,
      }),
      `Ids: project ${report.board.project}, board ${report.board.id}, page ${page.id}.`,
      read.board.unsaved ? "The window has edits it hasn't saved yet." : null,
      selectionText(page, report.selection),
      `The page has ${page.cards.length} cards. In view: ${report.visible.length} items${listed.length < report.visible.length ? ` (the first ${listed.length} listed)` : ""}${report.visible.length > 0 ? ":" : "."}`,
      ...listed.map((id) => `- ${describeItem(page, id)}`),
      "Next: read_page reads the whole page. From now on, pass these board and page ids to other tools.",
    );
  },
});
