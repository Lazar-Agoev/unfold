/**
 * A link being drawn by hand (`startLink`): the hint over the canvas while the person picks where it
 * ends (Escape lets go), then the label an `other` connection says what it is with. A link drawn from a
 * call leaves from it (its step). Drawing it saves at once as one undo step and selects it; what the
 * board refuses comes back as a message.
 */

import { useEffect } from "react";
import { PromptDialog, showToast } from "../components";
import { isTyping } from "../lib/typing";
import { useAuthor } from "../state/author";
import { cancelLink, drawLink, select, usePageView, type PageTarget } from "../state/boards";

/** Why a label can't be used: a drawn link says what it is. */
function labelProblem(text: string): string | null {
  return text.trim() === "" ? "Say what it is in a few words." : null;
}

export function LinkDrawing({ target }: { readonly target: PageTarget }) {
  const linking = usePageView(target, (view) => view.linking);
  const author = useAuthor();
  const picking = linking !== null && linking.to === null;

  // Escape lets go while the end is being picked (the label's dialog has its own Escape).
  useEffect(() => {
    if (!picking) {
      return undefined;
    }
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented && !isTyping(event.target)) {
        event.preventDefault();
        cancelLink(target);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
    };
  }, [picking, target]);

  if (!linking) {
    return null;
  }
  const { from, step, to } = linking;
  return (
    <>
      {to === null ? (
        <p className="board-hint" role="status">
          Click what it links to · <kbd>esc</kbd> cancels
        </p>
      ) : null}
      <PromptDialog
        open={to !== null}
        title="Draw link"
        description="A drawn link is Other until you pick its kind."
        label="Label"
        confirmLabel="Draw"
        placement="canvas"
        validate={labelProblem}
        onCancel={() => {
          cancelLink(target);
        }}
        onSubmit={(label) => {
          cancelLink(target);
          if (to !== null && author !== null) {
            void drawLink(target, { from, ...(step !== undefined ? { step } : {}), to, label: label.trim(), author }).then((outcome) => {
              if (outcome.ok) {
                select(target, { kind: "link", linkId: outcome.value });
              } else {
                showToast({ title: "Couldn't draw the link", body: outcome.message, tone: "danger" });
              }
            });
          }
          return null;
        }}
      />
    </>
  );
}
