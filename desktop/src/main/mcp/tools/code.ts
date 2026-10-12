/**
 * Read the code (design §6.4): `read_code` and `get_relations`.
 *
 * `read_code` reads a card's models (`card`), a type's definition (`type_definition`), a function's
 * owned edges (`relations`, whose targets come named) and their labels (`line_labels`), as the board
 * draws them: each Flow step with its focus (the exact expression or statement View code highlights)
 * and whether its call or callbacks expand in place. Source is shown only when the bytes on disk are
 * the ones Unfold compiled, as a card does. `get_relations` pages callers (`relations`) or a type's uses (`type_uses`), and lists the
 * board pages that show it (`board_refs`).
 */

import { z } from "zod";
import type { ToolContext } from "../context";
import type {
  BlockHeader,
  CodeText,
  CardView,
  CodeView,
  FlowModel,
  FlowNode,
  LabeledLine,
  OutgoingCall,
  ProjectRow,
  Relations,
  SourceFocus,
  TypeDefinition,
  TypeMember,
} from "../engine-types";
import { clip, count, keyName, labelText, place, typeText } from "../format";
import { cant, cursorOffset, fittingLines, textResult } from "../results";
import { codeTarget } from "../target";
import { defineTool } from "../tool";

/** The most source lines one result shows; longer functions come in pages. */
const MAX_LINES = 150;
/** The most characters of source one page shows, leaving room for the steps and calls of its lines. */
const MAX_SOURCE_CHARS = 12_000;
const TYPE_KINDS: ReadonlySet<string> = new Set(["interface", "type_alias", "enum", "class"]);
/** Kinds whose `throw`s belong to themselves, not to the block that writes them. */
const CALLABLE_KINDS: ReadonlySet<string> = new Set(["function", "method", "constructor", "getter", "setter", "class"]);

type EntranceCard = Extract<CardView, { type: "entrance" }>;

/** Lines `[from, to]` within a block's lines, one page long when not asked for. */
function shownLines(header: Pick<BlockHeader, "startLine" | "endLine">, lines: readonly [number, number] | undefined): [number, number] {
  if (lines === undefined) {
    return [header.startLine, Math.min(header.endLine, header.startLine + MAX_LINES - 1)];
  }
  const from = Math.max(header.startLine, lines[0]);
  return [from, Math.min(header.endLine, lines[1], from + MAX_LINES - 1)];
}

/** A source line's text, without the indentation the block's lines share (after the first). */
function sourceLines(source: CodeText): Map<number, string> {
  return new Map(
    source.lines.map((line, index) => {
      const text = source.text.slice(line.start, line.end);
      const dedent = index === 0 ? 0 : Math.min(source.dedent, line.indent);
      return [line.number, text.slice(dedent)];
    }),
  );
}

function numbered(lines: ReadonlyMap<number, string>, [from, to]: readonly [number, number]): string[] {
  const width = String(to).length;
  return [...lines].filter(([number]) => number >= from && number <= to).map(([number, text]) => `${String(number).padStart(width)} | ${text}`);
}

/** The block's own `throw`s (not those of the callbacks and classes written inside it), with their lines. */
function directThrows(source: CodeText, text: ReadonlyMap<number, string>): string[] {
  const { outline } = source;
  const ownedHere = (index: number): boolean => {
    const parent = outline[index]?.parent;
    return parent === undefined || (!CALLABLE_KINDS.has(outline[parent]?.kind ?? "") && ownedHere(parent));
  };
  return outline.flatMap((item, index) => (item.kind === "throw" && ownedHere(index) ? [`line ${item.startLine}: ${clip((text.get(item.startLine) ?? "").trim())}`] : []));
}

/**
 * Each source line's text as the file has it (not dedented), by line number: focus columns count in it.
 * The block's slice starts at the block, partway through its first line, so that line is `first`, the
 * file's whole line; without it, the first line is left out and a focus on it quotes nothing.
 */
function fileLines(source: CodeText, first: string | undefined): Map<number, string> {
  const lines = new Map(source.lines.slice(1).map((line) => [line.number, source.text.slice(line.start, line.end)]));
  const number = source.lines[0]?.number;
  if (number !== undefined && first !== undefined) {
    lines.set(number, first);
  }
  return lines;
}

/** The file's whole line `line`, when the file on disk is still the one Unfold compiled. */
async function wholeLine(context: ToolContext, project: ProjectRow, path: string, line: number): Promise<string | undefined> {
  const view = await context.engine("file_view", { project: project.id, path, range: { startLine: line, endLine: line } }).catch(() => null);
  const shown = view?.graph === "current" ? view.lines.find((entry) => entry.number === line) : undefined;
  return view && shown ? view.text.slice(shown.start - view.textStart, shown.end - view.textStart) : undefined;
}

/**
 * A step's focus, the exact source View code highlights: `27:7–27:46`, columns 1-based and inclusive,
 * with the expression when it is on one line.
 */
function focusText(focus: SourceFocus, lines: ReadonlyMap<number, string>): string {
  const range = `${focus.startLine}:${focus.startColumn + 1}–${focus.endLine}:${focus.endColumn}`;
  const text = focus.startLine === focus.endLine ? lines.get(focus.startLine)?.slice(focus.startColumn, focus.endColumn).trim() : undefined;
  return text !== undefined && text.length > 0 ? `${range} “${clip(text, 80)}”` : range;
}

/** Whether a step's main call and callbacks expand in place (a resolved call, any callback), or why the call can't. */
function expansionLines(node: FlowNode): string[] {
  const call = node.call === undefined ? undefined : node.chips?.[node.call];
  const lines: string[] = [];
  if (call?.resolution === "resolved" && call.target !== undefined) {
    lines.push(`  expands in place: the call ${call.name} (its expandCalls key ${call.source}; callee key ${call.target})`);
  } else if (call) {
    const why = call.resolution === "external" ? `is external${call.note !== undefined ? ` (${call.note})` : ""}` : `is unresolved${call.note !== undefined ? `: ${call.note}` : ""}`;
    lines.push(`  doesn't expand in place: the call ${call.name} ${why}`);
  }
  for (const callback of node.callbacks ?? []) {
    lines.push(`  expands in place: the callback “${callback.label}” (its expandCalls key ${callback.key}; a callback that makes one resolved call shows that callee, through the callback)`);
  }
  return lines;
}

/** The Flow's steps between Start and End, within the lines shown: each with its focus, and what expands in place. */
function flowSteps(flow: FlowModel, [from, to]: readonly [number, number], lines: ReadonlyMap<number, string>): string[] {
  return flow.nodes.flatMap((node) => {
    if (node.kind === "start" || node.kind === "end" || node.key === undefined || node.startLine === undefined || node.startLine < from || node.startLine > to) {
      return [];
    }
    const span = node.endLine !== undefined && node.endLine !== node.startLine ? `${node.startLine}–${node.endLine}` : `${node.startLine}`;
    const name = node.fallback === true ? `fallback name “${node.label}”` : `“${node.label}”`;
    const focus = node.focus !== undefined ? `; focus ${focusText(node.focus, lines)}` : "";
    return [`- ${span} ${node.kind}${node.lane > 0 ? ` (branch lane ${node.lane})` : ""}: ${name}; key ${node.key}${focus}`, ...expansionLines(node)];
  });
}

const RELATION: Readonly<Record<OutgoingCall["kind"], string>> = { calls: "calls", constructs: "constructs", imports: "imports", references: "passes" };

/** One owned edge: where it goes, its typed label, and how to name it for read_call. */
function callLine(call: OutgoingCall, names: Relations["targetNames"], labels: ReadonlyMap<string, LabeledLine>): string {
  const where =
    call.resolution === "resolved" && call.target !== undefined
      ? `${names[call.target] ?? keyName(call.target)} (key ${call.target})`
      : call.resolution === "external"
        ? `external, ${call.note ?? call.module ?? "a library"}`
        : `unresolved: ${call.note ?? call.reason ?? "unknown"}`;
  const label = labels.get(`${call.source}\u0000${call.seq}`)?.label;
  const typed = label ? `, ${labelText(label)}` : "";
  return `- line ${call.line}: ${RELATION[call.kind]} ${call.text} → ${where}${typed}; source ${call.source}, seq ${call.seq}`;
}

/** The owned edges within the lines shown, in source order, each with its label. */
async function callLines(context: ToolContext, project: ProjectRow, relations: Relations, [from, to]: readonly [number, number]): Promise<string[]> {
  const calls = [...relations.outgoing.onBoard, ...relations.outgoing.openable, ...relations.outgoing.externalOrUnresolved]
    .filter((call) => call.line >= from && call.line <= to)
    .sort((a, b) => a.line - b.line || a.source.localeCompare(b.source) || a.seq - b.seq);
  const resolved = calls.filter((call) => call.resolution === "resolved" && call.kind !== "imports").map((call) => ({ source: call.source, seq: call.seq }));
  const labeled = resolved.length > 0 ? await context.engine("line_labels", { project: project.id, lines: resolved }) : [];
  const labels = new Map(labeled.map((line) => [`${line.source}\u0000${line.seq}`, line]));
  return calls.map((call) => callLine(call, relations.targetNames, labels));
}

function memberLine(member: TypeMember): string {
  const name = `${member.readonly ? "readonly " : ""}${member.name ?? `(${member.kind} signature)`}${member.optional ? "?" : ""}`;
  if (member.kind === "value") {
    return `- ${name}${member.value !== undefined ? ` = ${member.value}` : ""}`;
  }
  const kind = member.kind === "property" ? "" : `${member.kind} `;
  return `- ${kind}${name}${member.type ? `: ${typeText(member.type)}` : ""}${member.default ? " (has a default)" : ""}`;
}

/** A type's or class's written members, inherited members by base, aliased type, and uses. */
function definitionLines(definition: TypeDefinition): string[] {
  return [
    definition.aliased ? `Aliased type: ${typeText(definition.aliased)}` : null,
    definition.members.length > 0 ? `Members (${definition.members.length}):` : definition.aliased ? null : "No members are written.",
    ...definition.members.map(memberLine),
    ...definition.inherited.flatMap((group) => [
      `Inherited from ${typeText(group.from)}${group.members.length === 0 ? ": no members Unfold can read" : ` (${group.members.length}):`}`,
      ...group.members.map(memberLine),
    ]),
    `Used in ${count(definition.uses, "place")} (get_relations lists them).`,
  ].filter((line): line is string => line !== null);
}

/** What changed source means for the reader. */
function sourceNote(header: Pick<BlockHeader, "source">): string | null {
  switch (header.source) {
    case "current":
      return null;
    case "changed":
      return "The file changed since the last compile, so Unfold shows no source, steps, or throws: compile_project, then read it again.";
    case "missing":
      return "The file is gone since the last compile: compile_project, then search for the code's new place.";
  }
}

function evidenceLines(evidence: EntranceCard["evidence"]): string[] {
  const where = place(evidence.path, evidence.startLine, evidence.endLine);
  if (evidence.source !== "current") {
    return [`Declared in ${where}, which changed since the last compile: compile_project, then read it again.`];
  }
  const source: CodeView["source"] | null = evidence.code?.source ?? null;
  if (source?.state === "current") {
    return [`Declared at ${where}:`, ...numbered(sourceLines(source), [evidence.startLine ?? 0, evidence.endLine ?? Number.MAX_SAFE_INTEGER])];
  }
  if (evidence.text !== null) {
    const first = evidence.startLine ?? 1;
    return [`Declared at ${where}${evidence.pointer ? ` (${evidence.pointer})` : ""}:`, ...evidence.text.split("\n").map((line, index) => `${first + index} | ${line}`)];
  }
  return [`Declared in ${where}.`];
}

function entranceLines(card: EntranceCard): string[] {
  const { header } = card;
  return [
    `Entrance ${header.title} (${header.kind}, detector ${header.detector}${header.triggerStatus === "exact" ? "" : `, trigger ${header.triggerStatus}`}); key ${header.key}.`,
    card.applications.length > 0 ? `In ${card.applications.join(", ")}.` : "No launch reaches it.",
    ...evidenceLines(card.evidence),
    `${card.targets.some((target) => target.role === "startup") ? "Startup and handlers" : "Handlers"}, in order (${card.targets.length}):`,
    ...card.targets.map((target) => {
      const goes =
        target.resolution === "resolved" && target.target !== null
          ? `${target.name ?? keyName(target.target)} at ${place(target.path ?? "", target.line)} (key ${target.target})`
          : `${target.resolution}${target.note !== null ? `: ${target.note}` : ""}`;
      return `${target.position + 1}. ${target.role === "startup" ? "startup " : ""}${target.text} → ${goes}`;
    }),
    "Next: add_cards with this key puts it on a page with its handlers (follow: {callees: 1}); create_board with trace makes a board from it.",
  ];
}

const VIEWS = ["code", "flow", "both"] as const;

export const readCode = defineTool({
  name: "read_code",
  title: "Read code",
  description:
    'Reads a function, method, class, type, or entrance as Unfold compiled it, by key (from search, list_entrances, read_page, or get_relations) or a link to a card; without key, the card selected in the person\'s view. A function: its verified source with line numbers, its signature as written, its Flow steps (each with lines, kind, fallback name, key, and focus: the exact source View code highlights; the keys and lines group_steps, label_steps, and propose take; which calls and callbacks expandCalls can expand in place, and why an external or unresolved call can\'t), its direct throws, and each call it makes: resolved (the target\'s name and key, and its typed label such as (Cart, User) → Promise<Charge>), external (the library), or unresolved (the reason), with the source and seq read_call takes. view "code", "flow", or "both" (the default) picks source or steps. A type or class: its members with written types and what each type name links to, inherited members by base, and how many places use it. An entrance: where it\'s declared and its startup and handlers, in order. Unfold shows source only when the file is the one it compiled; otherwise it says the file changed: compile_project, then read again. Long functions come in pages of at most 150 lines (fewer when lines are long): pass lines [from, to]. Reading changes nothing.',
  input: z.object({
    key: z.string().min(1).optional().describe("The code's key, or a link to a card (default: the selected card)"),
    project: z.string().min(1).optional().describe("The project's name or id (default: the project of the board in view)"),
    view: z.enum(VIEWS).default("both").describe("code: source and calls; flow: steps; both"),
    lines: z
      .tuple([z.number().int().min(1), z.number().int().min(1)])
      .refine(([from, to]) => from <= to, "lines is [from, to] with from ≤ to")
      .optional()
      .describe("A range of the function's lines, 1-based and inclusive"),
  }),
  annotations: { readOnlyHint: true },
  async run({ key, project, view, lines }, context) {
    const code = await codeTarget(context, { key, project });
    const card = await context.engine("card", { project: code.project.id, key: code.key, lens: "both" });
    const head = context.header({ project: code.project.name });
    if (card.type === "entrance") {
      return textResult(head, ...entranceLines(card));
    }
    const { header } = card;
    if (lines !== undefined && (lines[1] < header.startLine || lines[0] > header.endLine)) {
      throw cant("read those lines", `${header.qualifiedName} spans lines ${header.startLine}–${header.endLine}`, "Pass lines within them, or leave lines out for the first page.");
    }
    const isType = TYPE_KINDS.has(header.kind);
    const [definition, relations] = await Promise.all([
      isType ? context.engine("type_definition", { project: code.project.id, key: code.key }) : null,
      header.kind === "class" || !isType ? context.engine("relations", { project: code.project.id, key: code.key, callersLimit: 1 }) : null,
    ]);
    const asked = shownLines(header, lines);
    const source = card.code?.source.state === "current" ? card.code.source : null;
    const text = source ? sourceLines(source) : new Map<number, string>();
    const showCode = view !== "flow";
    // A page of long lines ends early, so the size guard never cuts where the next page starts.
    const shown = source && showCode ? fittingLines(numbered(text, asked), MAX_SOURCE_CHARS).length : 0;
    const range: [number, number] = shown > 0 ? [asked[0], asked[0] + shown - 1] : asked;
    const showFlow = view !== "code" && !isType;
    const calls = relations && showCode ? await callLines(context, code.project, relations, range) : [];
    const throws = source && !isType ? directThrows(source, text) : [];
    const flow = showFlow && header.source === "current" ? card.flow : null;
    // Only a focus on the first line needs the file's whole line, to quote its expression.
    const firstLine = source?.lines[0]?.number;
    const onFirst = firstLine !== undefined && firstLine >= range[0] && flow?.nodes.some((node) => node.focus?.startLine === firstLine && node.focus.endLine === firstLine) === true;
    const first = onFirst ? await wholeLine(context, code.project, header.path, firstLine) : undefined;
    const steps = flow ? flowSteps(flow, range, source ? fileLines(source, first) : new Map()) : [];
    const paged = range[0] > header.startLine || range[1] < header.endLine;
    return textResult(
      head,
      `${header.kind} ${header.qualifiedName} at ${place(header.path, header.startLine, header.endLine)}; key ${header.key}.`,
      code.step !== undefined ? `Selected in the person's view: the step ${code.step}.` : null,
      sourceNote(header),
      source ? `Signature: ${clip(text.get(header.startLine)?.trim() ?? "", 200)}` : null,
      source && showCode ? `Source${paged ? `, lines ${range[0]}–${range[1]} of ${header.startLine}–${header.endLine}` : ""}:\n${numbered(text, range).join("\n")}` : null,
      definition ? definitionLines(definition).join("\n") : null,
      steps.length > 0
        ? `Flow steps (paths from written structure; names are fallbacks until labeled; focus is the step's exact source, line:column in the file, which View code highlights):\n${steps.join("\n")}`
        : null,
      throws.length > 0 ? `Throws directly:\n${throws.map((line) => `- ${line}`).join("\n")}` : source && !isType ? "Throws nothing directly." : null,
      calls.length > 0 ? `Calls (${calls.length}):\n${calls.join("\n")}` : null,
      relations && !isType ? `Called from ${count(relations.callersTotal, "place")}.` : null,
      range[1] < header.endLine ? `More: read_code with lines [${range[1] + 1}, ${Math.min(header.endLine, range[1] + MAX_LINES)}].` : null,
      `Next: read_call with a call's source for what it sends; get_relations for callers${isType ? " and uses" : ""}; add_cards puts it on a page.`,
    );
  },
});

// ---------------------------------------------------------------------------------------------
// get_relations

export const getRelations = defineTool({
  name: "get_relations",
  title: "Get relations",
  description:
    "How a function, method, class, or type connects: who calls it (each caller's name, file:line, and key, paged), what it calls (resolved, external, or unresolved), which entrances reach it (at least: what the graph proves), and which board pages already show it. For a type or class: the places that use it (each annotation's owner, file:line, and key, with extends and implements marked). key comes from search, read_code, read_page, or a link to a card; without key, the card selected in the person's view. limit sets a page of callers or uses (default 20, at most 100); pass the next cursor for more. Use it to choose what goes on a page, and before drawing a connection, to check whether Unfold already draws that call. Reading changes nothing.",
  input: z.object({
    key: z.string().min(1).optional().describe("The code's key, or a link to a card (default: the selected card)"),
    project: z.string().min(1).optional().describe("The project's name or id (default: the project of the board in view)"),
    limit: z.number().int().min(1).max(100).default(20).describe("Callers or uses per page"),
    cursor: z.string().optional().describe("The cursor an earlier page gave"),
  }),
  annotations: { readOnlyHint: true },
  async run({ key, project, limit, cursor }, context) {
    const code = await codeTarget(context, { key, project });
    const offset = cursorOffset(cursor);
    const id = code.project.id;
    const entrance = code.key.startsWith("entrance:");
    const kind = entrance ? "entrance" : (/[#/](\w+):[^/]*$/.exec(code.key)?.[1] ?? "");
    const isType = TYPE_KINDS.has(kind);
    const [relations, uses, boards] = await Promise.all([
      entrance || (isType && kind !== "class") ? null : context.engine("relations", { project: id, key: code.key, callersOffset: offset, callersLimit: limit }),
      isType ? context.engine("type_uses", { project: id, key: code.key, offset, limit }) : null,
      context.engine("board_refs", { project: id, key: code.key }),
    ]);
    const callers = relations
      ? [
          `Called from ${count(relations.callersTotal, "place")}${relations.callers.length < relations.callersTotal ? ` (${offset + 1}–${offset + relations.callers.length} here)` : ""}:`,
          ...relations.callers.map(
            (caller) => `- ${caller.ownerName} at ${place(caller.path, caller.line)}${caller.kind === "calls" ? "" : `, ${caller.kind === "references" ? `passed as argument ${(caller.position ?? 0) + 1}` : caller.kind}`} (key ${caller.owner}; source ${caller.source})`,
          ),
        ]
      : [];
    const calls = relations ? [...relations.outgoing.onBoard, ...relations.outgoing.openable, ...relations.outgoing.externalOrUnresolved].sort((a, b) => a.line - b.line) : [];
    const usesLines = uses
      ? [
          `Used in ${count(uses.total, "place")}${uses.places.length < uses.total ? ` (${offset + 1}–${offset + uses.places.length} here)` : ""}:`,
          ...uses.places.map((use) => `- ${use.label} at ${place(use.path, use.line)}${use.heritage ? `, ${use.heritage} it` : ""} (key ${use.owner})`),
        ]
      : [];
    const more = (relations?.callersNext ?? null) !== null || (uses !== null && offset + limit < uses.total);
    return textResult(
      context.header({ project: code.project.name }),
      `${keyName(code.key)}: key ${code.key}.`,
      entrance ? "An entrance: read_code lists its handlers; list_entrances, what else starts the project." : null,
      ...callers,
      calls.length > 0 && offset === 0 ? [`Calls (${calls.length}):`, ...calls.map((call) => callLine(call, relations?.targetNames ?? {}, new Map()))].join("\n") : null,
      relations && offset === 0 ? (relations.reachedFrom.length > 0 ? `Reached at least from: ${relations.reachedFrom.map((from) => `${from.title} (key ${from.key})`).join(", ")}.` : "No entrance reaches it in the graph.") : null,
      ...usesLines,
      boards.refs.length > 0
        ? [`On boards${boards.truncated ? " (the first 50)" : ""}:`, ...boards.refs.map((ref) => `- ${ref.title} › ${ref.pageTitle} (board id ${ref.board}, page id ${ref.page})`)].join("\n")
        : "No board shows it yet.",
      more ? `More: get_relations with cursor "${offset + limit}".` : null,
      "Next: read_code with a key; add_cards puts code on a page.",
    );
  },
});
