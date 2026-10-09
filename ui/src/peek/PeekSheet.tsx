/** Placeholder peek sheet (U1c): slides over the content with the file's path, until U4 builds it. */

import type { JSX } from "react";
import { Sheet } from "../components";
import { useLayout } from "../state/layout";
import { closePeek, usePeekTarget } from "../state/overlays";

export function PeekSheet(): JSX.Element | null {
  const target = usePeekTarget();
  const pinned = useLayout((layout) => layout.peek.pinned);
  return target ? (
    <Sheet open label={`Source of ${target.path}`} pinned={pinned} onClose={closePeek}>
      <p>{target.path}</p>
    </Sheet>
  ) : null;
}
