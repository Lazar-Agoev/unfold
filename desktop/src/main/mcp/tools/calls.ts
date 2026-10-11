/**
 * `read_call` (design §6.4, §8.7): one call's contract (`call_contract`), as the board's line
 * contract shows it: what it sends, what comes back and where it lands, what the callee throws, and
 * whether the caller catches it. Written types only; nothing is inferred.
 */

import { z } from "zod";
import type { ToolContext } from "../context";
import type { CallContract, ContractReturn, ContractSend, ProjectRow } from "../engine-types";
import { count, keyFile, keyName, place, typeText } from "../format";
import { cant, textResult, ToolError } from "../results";
import { currentView, projectOrView } from "../target";
import { defineTool } from "../tool";

/** The most calls of a merged Diagram arrow one result reads. */
const MAX_CALLS = 8;

/** A call by its occurrence and edge. */
interface CallRef {
  readonly project: ProjectRow;
  readonly source: string;
  readonly seq: number;
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

function landsText(returns: ContractReturn): string {
  switch (returns.binding) {
    case "variable":
      return `lands in the variable ${returns.text ?? ""}`.trimEnd();
    case "return":
      return "is returned by the caller";
    case "argument":
      return `is passed to ${returns.text ?? "another call"}`;
    case "discard":
      return "is discarded";
    case "other":
      return "is used in an expression";
  }
}

function contractLines(call: CallRef, contract: CallContract): string[] {
  const file = keyFile(contract.source) ?? "";
  const calleeFile = keyFile(contract.target) ?? "";
  const returns = contract.returns;
  return [
    `The call at ${place(file, contract.callerLine)} (source ${call.source}, seq ${call.seq}) → ${keyName(contract.target)} (key ${contract.target}).`,
    contract.sends.length > 0 ? `Sends (${contract.sends.length}):\n${contract.sends.map(sendLine).join("\n")}` : "Sends nothing.",
    `Returns ${returns.type ? typeText(returns.type) : "no written type"}${returns.awaited ? ", awaited" : ""}; the value ${landsText(returns)}.`,
    contract.throws.length > 0
      ? `Throws directly (${contract.throws.length}): ${contract.throws.map((thrown) => `${typeText(thrown.type)} at ${place(calleeFile, thrown.line)}`).join("; ")}. ${contract.caught ? "The call sits in a try with a catch in the caller, so the caller catches them." : "The caller doesn't catch them around this call."}`
      : `${keyName(contract.target)} throws nothing directly.`,
    contract.calleeLines.length > 0 ? `Read from ${calleeFile} lines ${contract.calleeLines.join(", ")}.` : null,
  ].filter((line): line is string => line !== null);
}

/**
 * The calls `read_call` reads (the one named, else the line or merged arrow selected in the person's
 * view, at most {@link MAX_CALLS}), and how many the selection holds.
 */
async function callsOf(
  context: ToolContext,
  args: { readonly source?: string | undefined; readonly seq: number; readonly project?: string | undefined },
): Promise<{ readonly calls: CallRef[]; readonly total: number }> {
  if (args.source !== undefined) {
    return { calls: [{ project: await projectOrView(context, args.project), source: args.source, seq: args.seq }], total: 1 };
  }
  const selection = currentView(context)?.selection;
  if (selection?.kind === "line" || selection?.kind === "calls") {
    const project = await context.project(selection.project);
    const selected = selection.kind === "line" ? [selection] : selection.calls;
    return { calls: selected.slice(0, MAX_CALLS).map((call) => ({ project, source: call.source, seq: call.seq })), total: selected.length };
  }
  throw cant(
    "tell which call you mean",
    "no source was given and no line is selected in Unfold",
    "Pass source (an occurrence key from read_code's calls) and seq, or ask the person to select the line.",
  );
}

export const readCall = defineTool({
  name: "read_call",
  title: "Read call",
  description:
    "One call's contract, as the board's line contract shows it: what the caller sends (each argument beside the parameter it fills and that parameter's written type, or why it fills none), what comes back (the callee's written return type, whether it's awaited, and where the value lands), what the callee throws directly, and whether the caller's try catches it. Name the call by source (an occurrence key from read_code's calls, such as src/orders.ts#function:createOrder/variable:order/call[0]) and seq (0, the call itself, is the default; a passed reference has its own). Without source, it reads the line selected in the person's view (every call of a merged Diagram arrow). Only a resolved call or construction has a contract; read_code says where the others go. Written types only: Unfold never infers a type, so a parameter without one says so. Reading changes nothing.",
  input: z.object({
    source: z.string().min(1).optional().describe("The occurrence's key (default: the selected line)"),
    seq: z.number().int().min(0).default(0).describe("The edge: 0 for the call itself"),
    project: z.string().min(1).optional().describe("The project's name or id (default: the project of the board in view)"),
  }),
  annotations: { readOnlyHint: true },
  async run({ source, seq, project }, context) {
    const { calls, total } = await callsOf(context, { source, seq, project });
    const contracts = await Promise.all(
      calls.map((call) =>
        context.engine("call_contract", { project: call.project.id, source: call.source, seq: call.seq }).catch((error: unknown) => {
          throw error instanceof ToolError && error.code === "not_found"
            ? cant("read the contract", `${call.source} (seq ${call.seq}) isn't a resolved call or construction`, "Only those have contracts; read_code lists each call with where it goes and its source and seq.")
            : error;
        }),
      ),
    );
    const [first] = calls;
    return textResult(
      context.header(first ? { project: first.project.name } : {}),
      total > 1 ? `The selected arrow draws ${count(total, "call")}${total > calls.length ? ` (the first ${calls.length} here)` : ""}:` : null,
      ...contracts.flatMap((contract, index) => {
        const call = calls[index];
        return call ? contractLines(call, contract) : [];
      }),
      "Next: read_code on the callee for its source; written types only, so say in chat what isn't written.",
    );
  },
});
