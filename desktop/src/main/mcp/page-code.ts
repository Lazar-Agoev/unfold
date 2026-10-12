/**
 * What the compiler says about a page's cards (design §6.5, `read_page`): each card's model (its name,
 * location, Code, and Flow) and each type card's definition, and from them what the board draws
 * between cards: one arrow per call occurrence whose target has a card on the page (with its typed
 * label), an entrance's arrows to its handlers, and type arrows from a type card's field to the card
 * of each type its written type names; and the steps of expanded cards still showing names from the
 * code. The rules are copies of the canvas's (`ui/src/canvas/arrows.ts`, `cardBody`, the store's
 * `shownView`, and the outline's count of unlabeled steps), so change both together: a page opens in
 * Diagram or Code, a card is compact or expanded in Diagram, a type card keeps its fields in every
 * view, arrows join cards of one project only, and reach the first card of a key. Two differences are
 * deliberate: calls are listed one by one where a Diagram node merges them, and a block whose source
 * changed reads its Flow's chips (the canvas keeps the last verified Code, which only a window has).
 */

import type { ToolContext } from "./context";
import type { Card, CardView, EdgeKind, LineLabel, Page, ProjectRow, TypeDefinition, TypeMember, TypeText, View } from "./engine-types";
import { keyKind } from "./format";
import { ToolError } from "./results";

/** Block kinds whose cards are type cards. */
const TYPE_KINDS: ReadonlySet<string> = new Set(["interface", "type_alias", "enum", "class"]);
/** The most lines one `line_labels` request labels. */
const MAX_LABELED = 500;

/** One card with what the engine says of it. */
export interface CardCode {
  readonly card: Card;
  /** The key it shows. */
  readonly key: string;
  /** The project it reads from; null when no project of the workspace has its shared name. */
  readonly project: ProjectRow | null;
  readonly model: CardView | null;
  /** A type card's definition, when the card shows its fields. */
  readonly definition: TypeDefinition | null;
  /** Why its code can't be read now, when it can't. */
  readonly problem?: string;
}

/** An arrow the board draws for a call or a handler. */
export interface CallArrow {
  readonly from: string;
  readonly to: string;
  readonly kind: EdgeKind | "handler";
  /** The call as written, or the handler as registered. */
  readonly text: string;
  readonly line: number | null;
  /** The occurrence and edge `read_call` takes (none for a handler). */
  readonly source?: string;
  readonly seq?: number;
  readonly label?: LineLabel;
}

/** A type arrow: from a field of a type card to the card of a type its written type names. */
export interface TypeArrow {
  readonly from: string;
  readonly to: string;
  /** The field's name (`=` for an alias's aliased type), with `from <Base>` for an inherited one. */
  readonly field: string;
  readonly type: TypeText;
}

/** A Flow step still showing its name from the code. */
export interface UnlabeledStep {
  readonly card: string;
  readonly key: string;
  readonly kind: string;
  /** Its fallback name. */
  readonly name: string;
  readonly line?: number;
}

/** A page's cards with their code, and what the board draws from it. */
export interface PageCode {
  readonly cards: ReadonlyMap<string, CardCode>;
  readonly arrows: readonly CallArrow[];
  readonly typeArrows: readonly TypeArrow[];
  readonly unlabeled: readonly UnlabeledStep[];
}

/** The key a card shows. */
export function cardKey(card: Pick<Card, "ref">): string {
  return "block" in card.ref ? card.ref.block : card.ref.entrance;
}

/** Whether a key is a type card's: a type declaration or a class, which shows its fields in every view. */
export function isTypeKey(key: string): boolean {
  return TYPE_KINDS.has(keyKind(key));
}

/** The lens a page opens in: Code for a page whose default is Code, else Diagram (a legacy Flow default too). */
export function pageLens(page: Pick<Page, "defaultView">): "diagram" | "code" {
  return page.defaultView === "code" ? "code" : "diagram";
}

/** Whether a card shows its detail in Diagram: its own view, else the page's default, isn't Diagram (a legacy Code card is expanded). */
export function cardExpanded(page: Pick<Page, "defaultView">, card: Pick<Card, "view">): boolean {
  return (card.view ?? page.defaultView) !== "diagram";
}

/** The view a card shows as its page opens (no lens picked): Code in the Code lens; in Diagram, Flow when expanded, else a compact node. */
function shownView(page: Pick<Page, "defaultView">, card: Pick<Card, "view">): View {
  if (pageLens(page) === "code") {
    return "code";
  }
  return cardExpanded(page, card) ? "flow" : "diagram";
}

/** A failed read's reason, for the card's line. */
function why(error: unknown): string {
  if (error instanceof ToolError) {
    return error.message;
  }
  throw error;
}

async function loadCard(context: ToolContext, card: Card, project: ProjectRow | null): Promise<CardCode> {
  const key = cardKey(card);
  if (!project) {
    return { card, key, project, model: null, definition: null, problem: `its project ${card.ref.project ?? ""} isn't in the workspace` };
  }
  const definition = isTypeKey(key)
    ? context.engine("type_definition", { project: project.id, key }).then(
        (found) => found,
        () => null,
      )
    : Promise.resolve(null);
  try {
    const model = await context.engine("card", { project: project.id, key });
    return { card, key, project, model, definition: await definition };
  } catch (error) {
    return { card, key, project, model: null, definition: await definition, problem: why(error) };
  }
}

/** The resolved occurrences a block card shows: its Code's call links, else its Flow's chips (as the canvas reads them). */
function occurrences(model: Extract<CardView, { type: "block" }>): { source: string; seq: number; target: string; kind: EdgeKind; text: string; line: number }[] {
  const code = model.code?.source;
  if (code?.state === "current") {
    return code.links.flatMap((link) =>
      link.resolution === "resolved" && link.target !== undefined ? [{ source: link.source, seq: 0, target: link.target, kind: link.kind, text: link.text, line: link.line }] : [],
    );
  }
  return (model.flow?.nodes ?? []).flatMap((node) =>
    (node.chips ?? []).flatMap((chip) =>
      chip.resolution === "resolved" && chip.target !== undefined ? [{ source: chip.source, seq: chip.seq, target: chip.target, kind: chip.kind, text: chip.name, line: chip.line }] : [],
    ),
  );
}

/** The call and handler arrows of one card. */
function cardArrows(code: CardCode, cardOf: (project: string, key: string) => string | undefined): CallArrow[] {
  const { model, project } = code;
  if (!model || !project) {
    return [];
  }
  if (model.type === "entrance") {
    return model.targets.flatMap((target) => {
      const to = target.resolution === "resolved" && target.target !== null ? cardOf(project.id, target.target) : undefined;
      return to === undefined || to === code.card.id ? [] : [{ from: code.card.id, to, kind: "handler" as const, text: target.name ?? target.text, line: target.line }];
    });
  }
  const seen = new Set<string>();
  return occurrences(model).flatMap((occurrence) => {
    const to = cardOf(project.id, occurrence.target);
    const id = `${occurrence.source}\u0000${occurrence.seq}\u0000${to ?? ""}`;
    if (to === undefined || to === code.card.id || seen.has(id)) {
      return [];
    }
    seen.add(id);
    return [{ from: code.card.id, to, kind: occurrence.kind, text: occurrence.text, line: occurrence.line, source: occurrence.source, seq: occurrence.seq }];
  });
}

/** One field row of a type card: its name and written type (`=` for an alias's aliased type). */
function fieldRows(definition: TypeDefinition, shownBase: (base: TypeText) => boolean): { field: string; type: TypeText }[] {
  const member = (prefix: string) => (item: TypeMember) => (item.type ? [{ field: `${prefix}${item.name ?? `(${item.kind})`}`, type: item.type }] : []);
  return [
    ...(definition.aliased ? [{ field: "=", type: definition.aliased }] : []),
    ...definition.members.flatMap(member("")),
    // A base with its own card on the page draws its own fields' arrows.
    ...definition.inherited.flatMap((group) => (shownBase(group.from) ? [] : group.members.flatMap(member(`from ${group.from.text}: `)))),
  ];
}

/** The type arrows of one type card showing its fields: one per field and card its written type names. */
function cardTypeArrows(code: CardCode, cardOf: (project: string, key: string) => string | undefined): TypeArrow[] {
  const { definition, project } = code;
  if (!definition || !project) {
    return [];
  }
  const declared = (type: TypeText) => type.spans.flatMap((span) => (span.style === "declared" && span.target !== undefined ? [span.target] : []));
  // A base is the type its text starts with (`Base<Item>` is Base).
  const shownBase = (base: TypeText) => {
    const [first] = base.spans;
    return first?.style === "declared" && first.target !== undefined && cardOf(project.id, first.target) !== undefined;
  };
  return fieldRows(definition, shownBase).flatMap(({ field, type }) => {
    const targets = new Set(declared(type).flatMap((key) => cardOf(project.id, key) ?? []));
    targets.delete(code.card.id);
    return [...targets].map((to) => ({ from: code.card.id, to, field, type }));
  });
}

/** Whether `key` is `member` or lies under it (`<member>/…`). */
function within(key: string, member: string): boolean {
  return key.startsWith(member) && (key.length === member.length || key[member.length] === "/");
}

/**
 * The keyed Flow steps of expanded cards that have no label on the page, as the page's outline counts
 * them: start and end aside, the steps a collapsed step group of the card hides aside, and none while
 * the source changed or the page opens in Code.
 */
function unlabeledSteps(page: Page, code: CardCode): UnlabeledStep[] {
  const flow = code.model?.type === "block" ? code.model.flow : null;
  if (flow?.source !== "current" || shownView(page, code.card) !== "flow") {
    return [];
  }
  const project = code.card.ref.project;
  const labeled = new Set(page.labels.filter((label) => label.project === project).map((label) => label.key));
  const hidden = page.groups.filter((group) => group.collapsed && group.card === code.card.id).flatMap((group) => group.members);
  return flow.nodes.flatMap(({ key, kind, label, startLine }) =>
    key === undefined || kind === "start" || kind === "end" || labeled.has(key) || hidden.some((member) => within(key, member))
      ? []
      : [{ card: code.card.id, key, kind, name: label, ...(startLine !== undefined ? { line: startLine } : {}) }],
  );
}

/** Reads a page's cards' code and works out what the board draws between them. */
export async function pageCode(context: ToolContext, own: ProjectRow, page: Page): Promise<PageCode> {
  const rows = await context.projects();
  const projectOf = (card: Card): ProjectRow | null => (card.ref.project === undefined ? own : (rows.find((row) => row.name === card.ref.project) ?? null));
  const loaded = await Promise.all(page.cards.map((card) => loadCard(context, card, projectOf(card))));
  const cards = new Map(loaded.map((code) => [code.card.id, code]));
  // A call or a type reaches the first card of its key, as the canvas draws it.
  const byKey = new Map<string, string>();
  for (const code of loaded) {
    const ref = code.project ? `${code.project.id}\u0000${code.key}` : undefined;
    if (ref !== undefined && !byKey.has(ref)) {
      byKey.set(ref, code.card.id);
    }
  }
  const cardOf = (project: string, key: string) => byKey.get(`${project}\u0000${key}`);
  const calls = loaded.flatMap((code) => cardArrows(code, cardOf));
  const arrows = await labeled(context, loaded, calls);
  return {
    cards,
    arrows,
    typeArrows: loaded.flatMap((code) => cardTypeArrows(code, cardOf)),
    unlabeled: loaded.flatMap((code) => unlabeledSteps(page, code)),
  };
}

/** The arrows with the typed labels of their calls (`line_labels`, per project); imports have none. */
async function labeled(context: ToolContext, loaded: readonly CardCode[], arrows: readonly CallArrow[]): Promise<CallArrow[]> {
  const projectOf = new Map(loaded.map((code) => [code.card.id, code.project?.id]));
  const byProject = new Map<string, CallArrow[]>();
  for (const arrow of arrows) {
    const project = projectOf.get(arrow.from);
    if (project !== undefined && arrow.source !== undefined && arrow.kind !== "imports") {
      const lines = byProject.get(project) ?? [];
      lines.push(arrow);
      byProject.set(project, lines);
    }
  }
  const labels = new Map<string, LineLabel>();
  const requests = [...byProject].flatMap(([project, lines]) =>
    Array.from({ length: Math.ceil(lines.length / MAX_LABELED) }, (_, chunk) => ({
      project,
      lines: lines.slice(chunk * MAX_LABELED, (chunk + 1) * MAX_LABELED).map((arrow) => ({ source: arrow.source ?? "", seq: arrow.seq ?? 0 })),
    })),
  );
  // Labels are what a line sends and returns; a page reads without them when they can't be read.
  const answers = await Promise.all(requests.map((request) => context.engine("line_labels", request).catch(() => [])));
  answers.forEach((lines, index) => {
    for (const line of lines) {
      if (line.label) {
        labels.set(`${requests[index]?.project ?? ""}\u0000${line.source}\u0000${line.seq}`, line.label);
      }
    }
  });
  return arrows.map((arrow) => {
    // Two projects can have the same keys: a label is its project's.
    const label = labels.get(`${projectOf.get(arrow.from) ?? ""}\u0000${arrow.source ?? ""}\u0000${arrow.seq ?? 0}`);
    return label ? { ...arrow, label } : arrow;
  });
}
