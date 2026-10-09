/**
 * Modal layers (dialogs, the command palette). While one is open, window shortcuts stay quiet so a
 * key never acts on the view behind the modal.
 */

let depth = 0;

/** Marks a modal as open. Returns the function that marks it closed again (idempotent). */
export function pushModal(): () => void {
  depth += 1;
  let open = true;
  return () => {
    if (open) {
      open = false;
      depth -= 1;
    }
  };
}

export function isModalOpen(): boolean {
  return depth > 0;
}
