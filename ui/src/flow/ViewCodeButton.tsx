/**
 * The small `</>` button that opens something's actual source in the peek sheet (View code): on a
 * hovered or selected Flow step, on a combined block, beside an entrance's declaration, and on each of
 * its resolved targets. A click never reaches the card under it.
 */

import { Tooltip } from "../components";
import "./flow.css";

export interface ViewCodeButtonProps {
  /** Its accessible name; "View code" unless it opens something other than the selection's source. */
  readonly label?: string;
  /** Whether it can be reached with Tab (it shows only on hover otherwise). */
  readonly tabbable?: boolean;
  readonly className?: string;
  readonly onView: () => void;
}

export function ViewCodeButton({ label = "View code", tabbable = true, className, onView }: ViewCodeButtonProps) {
  return (
    <Tooltip content={label} describe={false}>
      <button
        type="button"
        className={`view-code nodrag${className !== undefined ? ` ${className}` : ""}`}
        aria-label={label}
        tabIndex={tabbable ? undefined : -1}
        onClick={(event) => {
          event.stopPropagation();
          onView();
        }}
        onDoubleClick={(event) => {
          event.stopPropagation();
        }}
      >
        {"</>"}
      </button>
    </Tooltip>
  );
}
