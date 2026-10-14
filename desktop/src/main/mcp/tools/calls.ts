/**
 * `read_call`: one call's contract (`call_contract`), named by the call's block
 * key, as the board's arrow contract shows it: what it sends, what comes back and where it lands, what
 * the callee raises directly, and which part of the caller catches it. Written types only; nothing is
 * inferred.
 */

import { z } from "zod";
import type { ToolContext } from "../context";
import type { CallContract, ContractRaise, ContractSend, Landing, LinkEnd, ProjectRow } from "../engine-types";
import { count, typeText } from "../format";
import { cant, textResult, ToolError } from "../results";
import { currentView, projectOrView } from "../target";
import { defineTool } from "../tool";

/** The most calls of a selected arrow one result reads. */
const MAX_CALLS = 8;

/** A call by its block key. */
interface CallRef {
  readonly project: ProjectRow;
  readonly key: string;
}

function sendLine(send: ContractSend): string {
  const argument = send.argument;
  const written = argument
    ? `${argument.expand === "spread" ? "..." : argument.expand === "keywords" ? "**" : ""}${argument.keyword !== undefined ? `${argument.keyword}=` : ""}${argument.text ?? `argument ${argument.index + 1}`}`
    : "(no argument)";
  const parameter = send.parameter;
  if (parameter) {
    const name = `${parameter.name ?? "(a pattern)"}${parameter.optional ? "?" : ""}`;
    return `- ${written} → ${name}${parameter.type ? `: ${typeText(parameter.type)}` : " (no written type)"}${parameter.default ? ", has a default" : ""}`;
  }
  return `- ${written} → no proven parameter: ${send.unpaired?.note ?? "unknown"}`;
}

/** Where a call goes: the project definition, or the library, builtin, or why it is unknown. */
function calleeText(callee: LinkEnd): string {
  switch (callee.outcome) {
    case "project":
      return callee.key === undefined ? "a project definition" : `the project definition ${callee.key}`;
    case "library":
      return `the library ${callee.note ?? callee.module ?? "call"}`;
    case "builtin":
      return "a builtin";
    case "unknown":
      return `an unknown callee (${callee.note ?? callee.reason ?? "unresolved"})`;
  }
}

function landsText(lands: Landing): string {
  switch (lands.kind) {
    case "binds":
      return `lands in ${lands.target}`;
    case "exit":
      return `is the value of the ${lands.form} ${lands.exit}`;
    case "argument":
      return `is passed as argument ${lands.index + 1} of ${lands.call}`;
    case "receiver":
      return `is the receiver of ${lands.call}`;
    case "nowhere":
      return "is discarded";
  }
}

function raiseText(raise: ContractRaise): string {
  const what = raise.type ? typeText(raise.type) : (raise.value ?? "a value");
  return `${what} at line ${raise.line} (key ${raise.exit}), ${raise.caughtBy !== undefined ? `caught by ${raise.caughtBy}` : "not caught around this call"}`;
}

function contractLines(contract: CallContract): string[] {
  const { returns } = contract;
  return [
    `The call ${contract.text} at line ${contract.line} (key ${contract.call}) → ${calleeText(contract.callee)}.`,
    contract.sends.length > 0 ? `Sends (${contract.sends.length}):\n${contract.sends.map(sendLine).join("\n")}` : "Sends nothing.",
    `Returns ${returns.type ? typeText(returns.type) : "no written type"}${returns.awaited ? ", awaited" : ""}; the value ${landsText(returns.lands)}.`,
    contract.raises.length > 0 ? `Raises directly (${contract.raises.length}): ${contract.raises.map(raiseText).join("; ")}.` : "The callee raises nothing directly.",
  ];
}

/**
 * The calls `read_call` reads (the one named, else the calls of the arrow selected in the person's
 * view, at most {@link MAX_CALLS}), and how many the selection holds.
 */
async function callsOf(context: ToolContext, args: { readonly call?: string | undefined; readonly project?: string | undefined }): Promise<{ readonly calls: CallRef[]; readonly total: number }> {
  if (args.call !== undefined) {
    return { calls: [{ project: await projectOrView(context, args.project), key: args.call }], total: 1 };
  }
  const view = currentView(context);
  const selection = view?.selection;
  if (view?.board && selection?.kind === "arrow") {
    const project = await context.project(view.board.project);
    return { calls: selection.calls.slice(0, MAX_CALLS).map((key) => ({ project, key })), total: selection.calls.length };
  }
  throw cant("tell which call you mean", "no call was given and no arrow is selected in Unfold", "Pass call (a call's key from read_code), or ask the person to select the arrow.");
}

export const readCall = defineTool({
  name: "read_call",
  title: "Read call",
  description:
    "One call's contract, as the board's arrow contract shows it: what the caller sends (each argument beside the parameter it fills and that parameter's written type, or why it fills none), what comes back (the callee's written return type, whether it's awaited, and where the value lands), what the callee raises directly, and which part of the caller catches it. Name the call by its key (from read_code, such as shop/service.py#place_order/try[0]/body/call:charge_card). Without call, it reads the arrow selected in the person's view (every call it draws). Written types only: Unfold never infers a type, so a parameter without one says so; get_guide(\"graph\") says what a contract proves. Reading changes nothing.",
  input: z.object({
    call: z.string().min(1).optional().describe("The call's key (default: the selected arrow's calls)"),
    project: z.string().min(1).optional().describe("The project's name or id (default: the project of the board in view)"),
  }),
  annotations: { readOnlyHint: true },
  async run({ call, project }, context) {
    const { calls, total } = await callsOf(context, { call, project });
    const contracts = await Promise.all(
      calls.map((ref) =>
        context.engine("call_contract", { project: ref.project.id, key: ref.key }).catch((error: unknown) => {
          throw error instanceof ToolError && error.code === "not_found"
            ? cant("read the contract", `${ref.key} isn't a call Unfold compiled`, "read_code lists each call with its key.")
            : error;
        }),
      ),
    );
    const [first] = calls;
    return textResult(
      context.header(first ? { project: first.project.name } : {}),
      total > 1 ? `The selected arrow draws ${count(total, "call")}${total > calls.length ? ` (the first ${calls.length} here)` : ""}:` : null,
      ...contracts.flatMap(contractLines),
      "Next: read_code on the callee for its code; written types only, so say in chat what isn't written.",
    );
  },
});
