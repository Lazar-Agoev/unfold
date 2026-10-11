/**
 * The closed labels of a page's lines: one `line_labels` call per project for every call and
 * reference arrow on the page (in chunks of 500, the command's limit). Handler and type arrows have
 * none, and neither has an arrow that stands for several calls (it reads "3 calls").
 * Labels stay shown while a changed page asks again; `graph_updated` of the project asks again.
 */

import { keepPreviousData, useQueries } from "@tanstack/react-query";
import { useMemo } from "react";
import type { LineLabel, LineRef, ProjectId } from "../../ipc/generated";
import { engineQueryOptions } from "../../ipc/queries";
import type { Arrow } from "../arrows";

/** The most lines one `line_labels` call takes. */
export const LABELS_PER_CALL = 500;

/** A line's key in the result: its project, occurrence, and edge. */
export function lineKey(project: ProjectId, source: string, seq: number): string {
  return `${project}\u0000${source}\u0000${seq}`;
}

/** Whether an arrow draws one call or passed reference, whose line has a typed label. */
export function hasLineLabel(arrow: Arrow): boolean {
  return arrow.kind !== "handler" && arrow.kind !== "type" && arrow.calls === undefined;
}

/** The calls a page's arrows need: per project, its distinct lines in order, at most 500 per call. */
export function labelRequests(arrows: readonly Arrow[]): { project: ProjectId; lines: LineRef[] }[] {
  const byProject = new Map<ProjectId, Map<string, LineRef>>();
  for (const arrow of arrows) {
    if (!hasLineLabel(arrow)) {
      continue;
    }
    const lines = byProject.get(arrow.project) ?? new Map<string, LineRef>();
    lines.set(`${arrow.occurrence}\u0000${arrow.seq}`, { source: arrow.occurrence, seq: arrow.seq });
    byProject.set(arrow.project, lines);
  }
  const requests: { project: ProjectId; lines: LineRef[] }[] = [];
  for (const project of [...byProject.keys()].sort()) {
    const lines = [...(byProject.get(project)?.values() ?? [])].sort((a, b) => (a.source < b.source ? -1 : a.source > b.source ? 1 : a.seq - b.seq));
    for (let start = 0; start < lines.length; start += LABELS_PER_CALL) {
      requests.push({ project, lines: lines.slice(start, start + LABELS_PER_CALL) });
    }
  }
  return requests;
}

/** Each arrow's label by arrow id: a label, null when the line has none, absent while loading. */
export function useLineLabels(arrows: readonly Arrow[]): ReadonlyMap<string, LineLabel | null> {
  const requests = useMemo(() => labelRequests(arrows), [arrows]);
  const results = useQueries({
    queries: requests.map((request) => ({ ...engineQueryOptions("line_labels", request), placeholderData: keepPreviousData })),
    combine: (all) => all.map((result) => result.data),
  });
  return useMemo(() => {
    const byLine = new Map<string, LineLabel | null>();
    requests.forEach((request, index) => {
      for (const line of results[index] ?? []) {
        byLine.set(lineKey(request.project, line.source, line.seq), line.label);
      }
    });
    const labels = new Map<string, LineLabel | null>();
    for (const arrow of arrows) {
      const label = hasLineLabel(arrow) ? byLine.get(lineKey(arrow.project, arrow.occurrence, arrow.seq)) : undefined;
      if (label !== undefined) {
        labels.set(arrow.id, label);
      }
    }
    return labels;
  }, [arrows, requests, results]);
}
