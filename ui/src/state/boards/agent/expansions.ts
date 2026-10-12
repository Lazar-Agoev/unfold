/**
 * Calls expanded in place, as `set_view` checks them (docs/proposals/unified-code-diagrams.md §3): a
 * path of 1 to 3 keys, outermost first, each checked in the Flow of the block the level before leads
 * to (the card's own block first). A key is a step's main call that resolved, which leads to its
 * callee, or a callback written on a step, which leads to the callback itself, or, when the callback
 * is one call whose main call resolved (`() => sendEmail(job.data)`), to that callee unless it already
 * shows above (the canvas's rule, `callbackCallee`; then the callback itself). External and
 * unresolved calls, other keys, deeper paths, and a block that already shows above (a repeated
 * ancestor) are refused, each with what to pass instead. Pure: models come from a lookup, and a
 * model not loaded yet is asked for (`load`), so the edit's prepare step can load it and check again.
 */

import type { CardView, FlowModel, FlowNode, Key } from "../../../ipc/generated";
import { callbackCallee } from "../../../flow/insets";
import { keyDisplayName, MAX_EXPANSION_DEPTH } from "../document";

/** A model as loaded: null when the key isn't in the graph, undefined when it wasn't loaded yet. */
export type ModelLookup = (key: Key) => CardView | null | undefined;

/** What checking a call path found: the blocks its levels lead to, why it can't expand, or a model to load first. */
export type PathCheck =
  | { readonly ok: true; readonly blocks: readonly Key[] }
  | { readonly ok: false; readonly reason: string }
  | { readonly ok: "load"; readonly key: Key };

/** Where one key leads, or why it can't, or a model to load first. */
type Lead = { readonly ok: true; readonly block: Key } | { readonly ok: false; readonly reason: string } | { readonly ok: "load"; readonly key: Key };

/** What a path's keys are, for refusals. */
const WHAT_TO_PASS = "read_code marks which call steps and callbacks expand in place, with their keys";

/** The name a block goes by in a refusal: its qualified name, else one from its key. */
function blockName(key: Key, model: CardView | null | undefined): string {
  return model?.type === "block" ? model.header.qualifiedName : keyDisplayName(key);
}

/** A node's main call: the chip that makes it a call step. */
export function mainCall(node: Pick<FlowNode, "chips" | "call">): NonNullable<FlowNode["chips"]>[number] | undefined {
  return node.call === undefined ? undefined : node.chips?.[node.call];
}

/** Where `key` leads in the Flow of `block`, below the blocks `shown` (the card's own block first). */
function leadOf(block: Key, flow: FlowModel, key: Key, models: ModelLookup, shown: readonly Key[]): Lead {
  const name = blockName(block, models(block));
  for (const node of flow.nodes) {
    const main = mainCall(node);
    if (main?.source === key) {
      if (main.resolution === "resolved" && main.target !== undefined) {
        return { ok: true, block: main.target };
      }
      const why = main.resolution === "external" ? `external${main.note !== undefined ? ` (${main.note})` : ""}` : `unresolved${main.note !== undefined ? `: ${main.note}` : ""}`;
      return {
        ok: false,
        reason: `the call ${main.name} (line ${main.line}) in ${name} is ${why}, so it has no code to expand in place; its step keeps saying so. Expand a resolved call instead (${WHAT_TO_PASS}).`,
      };
    }
    const chip = node.chips?.find((item) => item.source === key);
    if (chip) {
      const step = main ? `its step's main call is ${main.name} (key ${main.source})` : "its step isn't a call step";
      return { ok: false, reason: `${chip.name} (line ${chip.line}) in ${name} is written inside another call: ${step}. Only a step's main call expands in place (${WHAT_TO_PASS}).` };
    }
    if (node.callbacks?.some((callback) => callback.key === key)) {
      const model = models(key);
      if (model === undefined) {
        return { ok: "load", key };
      }
      const callee = model?.type === "block" ? callbackCallee(model.flow, shown)?.target : undefined;
      return { ok: true, block: callee ?? key };
    }
  }
  return { ok: false, reason: `“${key}” isn't a call step's main call or a callback in ${name}'s Flow (${WHAT_TO_PASS}).` };
}

/**
 * Checks a call path of the card showing `root`: each key in the Flow of the block the level before
 * leads to, at most {@link MAX_EXPANSION_DEPTH} levels, never through a block already shown above.
 */
export function checkCallPath(root: Key, path: readonly Key[], models: ModelLookup): PathCheck {
  if (path.length === 0 || path.length > MAX_EXPANSION_DEPTH) {
    return {
      ok: false,
      reason: `a call path holds 1 to ${MAX_EXPANSION_DEPTH} keys, outermost first, and this one holds ${path.length}. Expand at most three levels: follow a deeper call to its own card (add_cards) instead.`,
    };
  }
  const shown: Key[] = [root];
  let block = root;
  for (const key of path) {
    const model = models(block);
    if (model === undefined) {
      return { ok: "load", key: block };
    }
    const flow = model?.type === "block" ? model.flow : null;
    if (flow?.source !== "current") {
      return {
        ok: false,
        reason: `${blockName(block, model)}'s Flow isn't available (its file changed since the last compile, it isn't in the graph, or it isn't a function), so nothing in it expands. Compile the project, then try again.`,
      };
    }
    const lead = leadOf(block, flow, key, models, shown);
    if (lead.ok !== true) {
      return lead;
    }
    if (shown.includes(lead.block)) {
      return {
        ok: false,
        reason: `${key} leads to ${blockName(lead.block, models(lead.block))}, which already shows above it (a repeated ancestor), so expanding stops there. Leave this path out; its follow link stays.`,
      };
    }
    shown.push(lead.block);
    block = lead.block;
  }
  return { ok: true, blocks: shown.slice(1) };
}
