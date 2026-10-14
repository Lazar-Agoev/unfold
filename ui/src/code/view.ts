/** The code panel's model for what it shows (`code_view`), and the line its selection marks. */

import { keepPreviousData, skipToken, useQuery, type UseQueryResult } from "@tanstack/react-query";
import { call, engineKey } from "../ipc";
import type { CodeView, CodeViewArgs } from "../ipc/generated";
import type { UnfoldError } from "../ipc/unfold";
import type { CodeOpen } from "../state/codePanel";

/** The `code_view` arguments for what the panel shows. */
export function codeViewArgs(open: CodeOpen): CodeViewArgs {
  return { project: open.project, target: open.target, ...(open.show ? { show: open.show } : {}) };
}

/** The view of what the panel shows, kept current (and kept while it is asked for again). */
export function useCodeView(open: CodeOpen | null): UseQueryResult<CodeView, UnfoldError> {
  const args = open ? codeViewArgs(open) : null;
  return useQuery<CodeView, UnfoldError>({
    queryKey: args ? engineKey("code_view", args) : ["engine", "code_view", null],
    queryFn: args ? () => call("code_view", args) : skipToken,
    placeholderData: keepPreviousData,
  });
}

/** The first line the selection marks (only when the lines can be trusted). */
export function markedLine(view: CodeView): number | undefined {
  return view.state === "current" ? (view.mark?.lines?.[0] ?? view.mark?.expression[0]?.line) : undefined;
}
