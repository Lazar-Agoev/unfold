/** A health problem on what the inspector shows (outdated, held for review, missing), with Review…. */

import { AlertTriangle } from "lucide-react";
import { openBoardDialog } from "../boards/dialogs";
import type { HealthItem } from "../ipc/generated";
import { kindWord } from "../state/boards/health";

export function IssueLine({ boardId, item }: { readonly boardId: string; readonly item: HealthItem }) {
  return (
    <span className={`inspector-issue kind-${item.kind}`}>
      <AlertTriangle size={11} aria-hidden="true" />
      <span>
        {kindWord(item.kind)}: {item.message}
      </span>
      {item.repairs.length > 0 ? (
        <button
          type="button"
          className="inspector-link"
          onClick={() => {
            openBoardDialog({ kind: "repair", boardId, item });
          }}
        >
          Review…
        </button>
      ) : null}
    </span>
  );
}
