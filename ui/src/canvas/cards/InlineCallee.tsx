/** A callee expanded inline under its calling line (⌥-click): its verified source, read-only. */

import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useMemo, useState } from "react";
import { CodeView } from "../../code/CodeView";
import { defaultFolds } from "../../code/outline";
import { shortPath } from "../../code/text";
import type { ProjectId } from "../../ipc/generated";
import { cardQuery } from "../cardData";

export function InlineCallee({
  project,
  targetKey,
  onClose,
}: {
  readonly project: ProjectId;
  readonly targetKey: string;
  readonly onClose: () => void;
}) {
  // The same cached model as the callee's own card, so expanding an open card costs nothing.
  const query = useQuery(cardQuery(project, targetKey));
  const view = query.data?.type === "block" ? query.data : undefined;
  const code = view?.code?.source.state === "current" ? view.code.source : undefined;
  const initialFolds = useMemo(() => (code ? defaultFolds(code.outline) : []), [code]);
  const [toggled, setToggled] = useState<ReadonlySet<string> | null>(null);
  const folds = toggled ?? new Set(initialFolds);
  return (
    <div className="inline-callee" onClick={(event) => { event.stopPropagation(); }}>
      <div className="code-inline-heading">
        <strong>{view?.header.qualifiedName ?? targetKey}</strong>
        {view ? (
          <span>
            {shortPath(view.header.path)}:{view.header.startLine}
          </span>
        ) : null}
        <button type="button" className="code-inline-close nodrag" aria-label="Close the expanded callee" onClick={onClose}>
          <X size={12} aria-hidden="true" />
        </button>
      </div>
      {code && view ? (
        <CodeView
          code={code}
          language={view.header.language}
          label={view.header.qualifiedName}
          folds={folds}
          onToggleFold={(key) => {
            const next = new Set(folds);
            if (next.has(key)) {
              next.delete(key);
            } else {
              next.add(key);
            }
            setToggled(next);
          }}
        />
      ) : (
        <p className="card-note">{query.isError ? "This callee isn't in the graph." : view ? "Source unavailable." : "Loading…"}</p>
      )}
    </div>
  );
}
