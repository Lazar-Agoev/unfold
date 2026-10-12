/**
 * Check boards (design §6.5, §7.7): `check` and `get_screenshot`. Both draw the page in the render
 * window, as the board tab draws it (unsaved edits included), and never move the person's view.
 * `check` adds the board's health (`board_health`, which needs the board's session, so a board no
 * window holds is opened in a background tab first) to the page checks, each with the call that fixes
 * it; `get_screenshot` returns a PNG as MCP image content.
 */

import { z } from "zod";
import type { AgentBoardRef, AgentFailure, AgentIdentity } from "../../../shared/agent-api";
import type { PageChecks } from "../../agents/render";
import type { ToolContext } from "../context";
import type { PageCheckKind } from "../../../shared/render-api";
import type { HealthItem, HealthKind, Page } from "../engine-types";
import { authorText, count } from "../format";
import { personWords, repairNames, targetId, targetText, type RepairName } from "../health";
import { textResult, ToolError, type ToolResult } from "../results";
import { isLink, readPage, resolveBoard } from "../target";
import { defineTool } from "../tool";

/** What to do about the render window's refusals, by code. */
const RENDER_HINTS: Readonly<Record<string, string>> = {
  empty_page: "add_cards puts code on it first.",
  item_not_found: "read_page lists the page's ids; step groups and links have no image of their own: take their card's.",
  item_hidden: "set_view expands the frame that holds it, or take that frame's image.",
  render_timeout: "Try again; take a frame's or a card's image of a large page.",
  render_failed: "Try again.",
};

function renderRefusal(failure: AgentFailure): ToolError {
  const hint = RENDER_HINTS[failure.code];
  return new ToolError(hint === undefined ? failure.message : `${failure.message} ${hint}`, failure.code);
}

const HEALTH_WORDS: Readonly<Record<HealthKind, string>> = {
  missing: "missing",
  outdated: "outdated",
  brokenGroup: "broken step group",
  review: "held for review",
  notConnected: "not connected",
  brokenLink: "broken link",
};

const CHECK_WORDS: Readonly<Record<PageCheckKind, string>> = {
  overlap: "overlap",
  crossings: "crossings",
  density: "density",
  unlabeled: "unlabeled steps",
  missingWords: "missing words",
  evidence: "evidence",
  strays: "strays",
  views: "views",
};

/** The `repair` call for one of an item's repairs, or why it is the person's to do (D6). */
function repairCall(page: Page, board: AgentBoardRef, item: HealthItem, repair: RepairName, agent: AgentIdentity): string {
  if (repair === "resign" || repair === "accept") {
    const words = personWords(page, item, repair);
    if (words) {
      return `${repair} is ${authorText(words.author, agent)}'s to do: they wrote ${words.what}`;
    }
  }
  const args = [
    `board: "${board.id}"`,
    `page: "${page.id}"`,
    `item: "${targetId(item.target)}"`,
    `repair: "${repair}"`,
    ...(item.anchor !== undefined ? [`anchor: ${item.anchor}`] : []),
    ...(repair === "reconnect" ? ["to: <the new key>"] : []),
  ];
  return `repair({${args.join(", ")}})`;
}

/** A page's health items and page checks, each with its fix. */
function checkLines(page: Page, board: AgentBoardRef, checks: PageChecks, agent: AgentIdentity): string[] {
  const health = checks.health.map((item) => {
    const anchor = item.anchor !== undefined ? `, anchor ${item.anchor}` : "";
    const fixes = repairNames(item).map((repair) => repairCall(page, board, item, repair, agent));
    return `- ${HEALTH_WORDS[item.kind]}: ${targetText(page, item.target)} (item ${targetId(item.target)}${anchor}): ${item.message}${fixes.length > 0 ? `\n  Fix: ${fixes.join(" · ")}` : ""}`;
  });
  const found = checks.checks.map((check) => `- ${CHECK_WORDS[check.kind]}: ${check.message}${check.items.length > 0 ? ` (ids ${check.items.join(", ")})` : ""}\n  Fix: ${check.suggestion}`);
  return [
    ...(health.length > 0 ? [`Health (${health.length}):`, ...health] : ["Health: no problems."]),
    ...(found.length > 0 ? [`Page checks (${found.length}):`, ...found] : ["Page checks: nothing found."]),
  ];
}

function clean(checks: PageChecks): boolean {
  return checks.health.length === 0 && checks.checks.length === 0;
}

/** Whether `check` reviews the whole board: a board named (by id or by a link without a page), and no page. */
async function wholeBoard(context: ToolContext, board: string | undefined, page: string | undefined): Promise<boolean> {
  if (board === undefined || page !== undefined) {
    return false;
  }
  return !isLink(board) || (await context.link(board)).page === undefined;
}

export const check = defineTool({
  name: "check",
  title: "Check",
  description:
    'Reviews a page, or every page of a board, as data any agent can act on: health problems (missing, outdated, held for review, broken step groups, broken links, not connected), each with the repair calls it allows; and the page checks: overlap, crossings, density (over 30 visible items on a page, 12 in a frame, 12 steps in an expanded card), unlabeled steps of expanded cards (not those a collapsed step group hides), missing words (frames, step groups, and resource cards without a description), evidence (connections and resource cards without anchors), strays (items with no line, connection, or frame), and views (many expanded cards on an overview page), each with the tool that fixes it. page is an id or a link (default: the page in view); a board without a page checks every page of it. Unfold draws the page as the person sees it, unsaved edits included, and opens a board no window holds in a background tab. Call it after each section you build, then get_screenshot, and fix what it finds with targeted edits; don\'t start the page over. Re-signing or accepting what a person wrote is theirs to do: the result says so. Reading changes nothing.',
  input: z.object({
    board: z.string().min(1).optional().describe("A board's id or a link; alone, every page of it is checked"),
    page: z.string().min(1).optional().describe("A page's id or a link (default: the page in view)"),
    project: z.string().min(1).optional().describe("The board's project, when the board's id isn't enough"),
  }),
  annotations: { readOnlyHint: true },
  async run({ board, page, project }, context) {
    const target = await resolveBoard(context, { board, page, project });
    if (await wholeBoard(context, board, page)) {
      const checked = await context.renderer.checkBoard({ board: target.board });
      if (!checked.ok) {
        throw renderRefusal(checked);
      }
      const reads = await Promise.all(checked.pages.map((checks) => readPage(context, target.board, checks.page)));
      const sections = checked.pages.map((checks, index) => {
        const read = reads[index];
        const lines = clean(checks) || !read ? ["Nothing to fix."] : checkLines(read.page, target.board, checks, context.agent);
        return [`Page “${checks.pageTitle}” (page id ${checks.page}${read ? `, rev ${read.revision}` : ""}):`, ...lines];
      });
      const problems = checked.pages.filter((checks) => !clean(checks)).length;
      return textResult(
        context.header({ project: target.project.name, board: { title: checked.title, draft: reads[0]?.draft ?? false } }),
        `Board id ${target.board.id}: ${count(checked.pages.length, "page")}, ${problems === 0 ? "nothing to fix" : `${count(problems, "page")} with something to fix`}.`,
        ...sections.flat(),
        problems === 0 ? "Next: get_screenshot of each page, to look at it the way the person will." : "Next: fix with the calls above, then check again and get_screenshot.",
      );
    }
    const pageId = target.page ?? (await readPage(context, target.board, undefined)).page.id;
    const checked = await context.renderer.check({ board: target.board, page: pageId });
    if (!checked.ok) {
      throw renderRefusal(checked);
    }
    const read = await readPage(context, target.board, pageId);
    return textResult(
      context.header({ project: target.project.name, board: { title: read.title, draft: read.draft }, page: read.page.title, revision: read.revision }),
      `Ids: board ${target.board.id}, page ${pageId}.`,
      ...(clean(checked) ? ["Nothing to fix: no health problems, and the page checks found nothing."] : checkLines(read.page, target.board, checked, context.agent)),
      clean(checked) ? "Next: get_screenshot, to look at it the way the person will." : "Next: fix with the calls above, then check again and get_screenshot.",
    );
  },
});

export const getScreenshot = defineTool({
  name: "get_screenshot",
  title: "Get screenshot",
  description:
    "A PNG of a page, or of one card, frame, resource card, or note on it, drawn the way Unfold draws it for the person (unsaved edits included): at full size when it fits, else smaller, and the result says so; then take a frame's or a card's image to read it closer. page is an id or a link (default: the page in view); item is an item's id or a link to it; scale 2 draws a sharper image. It draws offscreen and never moves the person's view. Look at it as the person will: judge each frame, expanded card, proposed step, and connection, give a one-line verdict in the chat, and fix problems with targeted edits.",
  input: z.object({
    board: z.string().min(1).optional().describe("A board's id or a link (default: the board in view)"),
    page: z.string().min(1).optional().describe("A page's id or a link (default: the page in view)"),
    project: z.string().min(1).optional().describe("The board's project, when the board's id isn't enough"),
    item: z.string().min(1).optional().describe("A card's, frame's, resource card's, or note's id, or a link to it (default: the whole page)"),
    scale: z.union([z.literal(1), z.literal(2)]).default(1).describe("1, or 2 for a sharper image"),
  }),
  annotations: { readOnlyHint: true },
  async run({ board, page, project, item, scale }, context): Promise<ToolResult> {
    const target = await resolveBoard(context, { board, page, project, item });
    const read = await readPage(context, target.board, target.page);
    const shot = await context.renderer.screenshot({ board: target.board, page: read.page.id, ...(target.item !== undefined ? { item: target.item } : {}), scale });
    if (!shot.ok) {
      throw renderRefusal(shot);
    }
    const what = target.item === undefined ? `The page (page id ${read.page.id})` : `The item ${target.item} on page ${read.page.id}`;
    return {
      content: [
        {
          type: "text",
          text: [
            context.header({ project: target.project.name, board: { title: read.title, draft: read.draft }, page: read.page.title, revision: read.revision }),
            `${what}, ${shot.width}×${shot.height} px.${shot.note !== undefined ? ` ${shot.note}` : ""}`,
            "Next: look at it as the person will, give a one-line verdict in the chat, and fix what you see with targeted edits.",
          ].join("\n"),
        },
        { type: "image", data: shot.png.toString("base64"), mimeType: "image/png" },
      ],
    };
  },
});
