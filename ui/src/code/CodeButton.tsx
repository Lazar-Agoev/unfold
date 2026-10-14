/**
 * `</>`: opens the code of what it sits on in the code panel. It sits on a
 * card's header, and on a block while the block is hovered or selected. A click on it never also
 * clicks what it sits on.
 */

import "./codeButton.css";

export function CodeButton({ className, onView }: { readonly className: string; readonly onView: () => void }) {
  return (
    <button
      type="button"
      className={`code-button nodrag ${className}`}
      aria-label="View code"
      title="View code"
      onClick={(event) => {
        event.stopPropagation();
        onView();
      }}
      onKeyDown={(event) => {
        // Enter and Space press the button, never also select what it sits on.
        event.stopPropagation();
      }}
    >
      {"</>"}
    </button>
  );
}
