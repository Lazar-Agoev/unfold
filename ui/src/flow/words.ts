/**
 * The words a drawn flow is shown with: a
 * call box's meta row, a frame's keyword, an exit's word and where it goes, a gap tick, a combined
 * step's counts. Each comes from the drawn flow's fields; the layout measures the same words the view
 * renders.
 */

import type { CallBox, CombinedStep, Form, GapTick, Lane, LinkEnd, Route } from "../ipc/generated";

/** Where a library, builtin, or unknown call goes, in words; empty for a project call. */
export function outcomeWords(to: LinkEnd): string {
  switch (to.outcome) {
    case "project":
      return "";
    case "library":
      return to.note ?? to.module ?? "library";
    case "builtin":
      return "builtin";
    case "unknown":
      return to.note !== undefined ? `unknown · ${to.note}` : "unknown";
  }
}

/** A call box's meta row: what its value is assigned to, the call it feeds, its tags, and (not a project call) where it goes. */
export interface CallMeta {
  readonly binds: string | null;
  readonly feeds: string | null;
  readonly tags: readonly string[];
  readonly why: string | null;
}

export function callMeta(box: CallBox): CallMeta {
  const tags = [box.awaited ? "await" : null, box.sometimes ? "sometimes" : null, box.form === "new" ? "new" : null].filter((tag) => tag !== null);
  const why = outcomeWords(box.to);
  return {
    binds: box.binds !== undefined ? `→ ${box.binds}` : null,
    feeds: box.feeds !== undefined ? `into ${box.feeds.name ?? box.feeds.callee}(…)` : null,
    tags,
    why: why.length > 0 ? why : null,
  };
}

/** Whether a call box has anything in its meta row. */
export function hasMeta(meta: CallMeta): boolean {
  return meta.binds !== null || meta.feeds !== null || meta.tags.length > 0 || meta.why !== null;
}

// Maps, not object literals: `constructor` is a form too.
const FRAME_WORDS = new Map<Form, string>([
  ["if", "If"],
  ["ternary", "Ternary"],
  ["switch", "Switch"],
  ["match", "Match"],
  ["for", "Loop"],
  ["while", "Loop"],
  ["do", "Loop"],
  ["comprehension", "Comprehension"],
  ["with", "With"],
  ["using", "Using"],
]);

/** A frame's keyword, by its control block's form. */
export function frameWord(form: Form): string {
  return FRAME_WORDS.get(form) ?? form;
}

const EXIT_WORDS = new Map<Form, string>([
  ["return", "Return"],
  ["raise", "Raise"],
  ["throw", "Throw"],
  ["break", "Break"],
  ["continue", "Continue"],
  ["yield", "Yield"],
]);

/** An exit's word, by its form. */
export function exitWord(form: Form): string {
  return EXIT_WORDS.get(form) ?? form;
}

/** Where one of an exit's routes goes, in words: `End`, `raises to the caller`, `leaves the loop`. */
function routeWords(route: Route, inset: boolean): string | null {
  let words: string | null;
  switch (route.kind) {
    case "return":
      words = inset ? "returns" : "End";
      break;
    case "raise":
      words = route.to === "caller" ? "raises to the caller" : "the handlers";
      break;
    case "break":
      words = route.leaves === "switch" ? "leaves the switch" : "leaves the loop";
      break;
    case "continue":
      words = "next item";
      break;
    default:
      words = null;
  }
  return words !== null && route.through.length > 0 ? `finally, then ${words}` : words;
}

/** Where an exit goes, in words (its routes' words, each once), or null when its routes are drawn as wires. */
export function exitDestination(routes: readonly Route[], form: Form, inset: boolean): string | null {
  const words = [...new Set(routes.map((route) => routeWords(route, inset)).filter((item) => item !== null))];
  if (words.length === 0 && form === "yield") {
    return "resumes here";
  }
  return words.length > 0 ? words.join(", ") : null;
}

/** A gap tick's words: `⋯ 3 lines · for loop`. */
export function gapWords(gap: GapTick): string {
  const lines = `⋯ ${gap.count} ${gap.count === 1 ? "line" : "lines"}`;
  return gap.hidden.length > 0 ? `${lines} · ${gap.hidden.map((hidden) => hidden.label).join(", ")}` : lines;
}

/** One item of a collapsed combined step's counts. */
export interface SummaryItem {
  readonly text: string;
  readonly tone: "count" | "call" | "unknown";
}

function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** A collapsed combined step's counts: its tests, loops, project calls, library calls, and unknown calls. */
export function combinedSummary(step: CombinedStep): SummaryItem[] {
  return [
    ...(step.conditions > 0 ? [{ text: count(step.conditions, "condition", "conditions"), tone: "count" as const }] : []),
    ...(step.loops > 0 ? [{ text: count(step.loops, "loop", "loops"), tone: "count" as const }] : []),
    ...step.calls.map((call) => ({ text: call.name ?? call.callee, tone: "call" as const })),
    ...(step.library > 0 ? [{ text: `${step.library} library`, tone: "count" as const }] : []),
    ...(step.unknown > 0 ? [{ text: `${step.unknown} unknown`, tone: "unknown" as const }] : []),
  ];
}

/** A way out of a collapsed combined step that isn't on to the next step: its verb and where it goes. */
export interface CombinedExit {
  readonly kind: "return" | "raise" | "break" | "continue";
  readonly verb: string;
  readonly destination: string;
}

const EXIT_VERBS = { return: "returns", raise: "raises", break: "breaks", continue: "continues" } as const;

/** The ways out of a collapsed combined step (its routes' exits, each once), in the order its routes list them. */
export function combinedExits(routes: readonly Route[], inset: boolean): CombinedExit[] {
  const exits: CombinedExit[] = [];
  for (const route of routes) {
    if (route.kind !== "return" && route.kind !== "raise" && route.kind !== "break" && route.kind !== "continue") {
      continue;
    }
    let destination: string;
    switch (route.kind) {
      case "return":
        destination = inset ? "the caller" : "End";
        break;
      case "raise":
        destination = route.to === "caller" ? "the caller" : "the handlers";
        break;
      case "break":
        destination = route.leaves === "switch" ? "after the switch" : "after the loop";
        break;
      case "continue":
        destination = "next item";
        break;
    }
    if (!exits.some((exit) => exit.kind === route.kind && exit.destination === destination)) {
      exits.push({ kind: route.kind, verb: EXIT_VERBS[route.kind], destination });
    }
  }
  return exits;
}

/** A part's section title (`Try`, `Except`, `Else`, `Finally`) and its note or clause. */
export interface SectionWords {
  readonly title: string;
  /** A handler's clause as written, without its keyword. */
  readonly clause: string | null;
  /** What the section is, when the clause doesn't say. */
  readonly note: string | null;
}

/** A part of a try (or a loop's `else`) as a section. */
export function sectionWords(lane: Lane, loop: boolean): SectionWords {
  switch (lane.form) {
    case "except":
    case "catch": {
      const written = lane.test?.text;
      const clause = written?.startsWith(lane.form) ? written.slice(lane.form.length).trim() : written;
      return { title: lane.form === "except" ? "Except" : "Catch", clause: clause !== undefined && clause.length > 0 ? clause : null, note: null };
    }
    case "else":
      return { title: "Else", clause: null, note: loop ? "when the loop ends without break" : "when the try body raised nothing" };
    case "finally":
      return { title: "Finally", clause: null, note: "runs on every way out of the try" };
    default:
      return { title: "Try", clause: null, note: null };
  }
}

/** A decision part's head when it has no test: `else`, `default`. */
export function headWords(lane: Lane): string {
  if (lane.test?.text !== undefined) {
    return lane.form === "case" ? `case ${lane.test.text}` : lane.test.text;
  }
  return lane.form === "default" ? "default" : lane.form;
}
