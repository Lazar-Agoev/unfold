/** "Used in N places": a popover of the places a type is written. */

import { useState } from "react";
import { ListRow, Popover } from "../components";
import type { ProjectId, TypeDefinition } from "../ipc/generated";
import { useEngineQuery } from "../ipc/queries";
import "./types.css";

/** `src/cart.ts` and 4 → `cart.ts:4`. */
function placeText(path: string, line: number): string {
  return `${path.slice(path.lastIndexOf("/") + 1)}:${line}`;
}

/** "Used in 6 places", "Used in 1 place". */
function usesText(count: number): string {
  return `Used in ${count} ${count === 1 ? "place" : "places"}`;
}

export function UsedIn({ project, definition }: { readonly project: ProjectId; readonly definition: TypeDefinition }) {
  const [anchor, setAnchor] = useState<HTMLButtonElement | null>(null);
  const uses = useEngineQuery("type_uses", { project, key: definition.key }, { enabled: anchor !== null });
  const close = () => {
    setAnchor(null);
  };
  return (
    <>
      <button
        type="button"
        className="type-uses-action"
        aria-haspopup="dialog"
        aria-expanded={anchor !== null}
        disabled={definition.uses === 0}
        onClick={(event) => {
          event.stopPropagation();
          setAnchor(anchor ? null : event.currentTarget);
        }}
      >
        {usesText(definition.uses)}
      </button>
      <Popover open={anchor !== null} anchor={anchor} onClose={close} label={`Where ${definition.name} is used`} width={320} className="type-uses">
        {uses.data ? (
          <>
            <div role="list">
              {uses.data.places.map((place) => (
                <ListRow
                  key={`${place.owner}:${place.path}:${place.line}`}
                  role="listitem"
                  className="type-uses-row"
                  label={<span className="type-uses-label">{place.label}</span>}
                  trailing={<span className="type-uses-place">{placeText(place.path, place.line)}</span>}
                />
              ))}
            </div>
            {uses.data.total > uses.data.places.length ? (
              <p className="type-uses-more">
                The first {uses.data.places.length} of {uses.data.total}
              </p>
            ) : null}
          </>
        ) : (
          <p className="type-uses-more">{uses.isError ? uses.error.message : "Looking…"}</p>
        )}
      </Popover>
    </>
  );
}
