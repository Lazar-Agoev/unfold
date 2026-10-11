/**
 * A line's closed label: the call's signature in small mono on the canvas color, so the line under it
 * is hidden and the text stays readable (docs/proposals/typed-lines.md §7). Like a drawn link's label
 * it sits in the edge-label layer, over the cards, so it must never cover one: it reports its curve
 * and the widths of its forms (measured with the bundled font) to the page's label layout, which
 * places it at the first free spot along the curve, in a shorter form if it must, or not at all (the
 * line then takes the click itself, see `ArrowEdge`). A click selects the line, which opens the
 * contract.
 */

import { useLayoutEffect, useMemo } from "react";
import type { LineLabel as LineLabelModel } from "../../ipc/generated";
import { LINE_LABEL_FONT, useMeasure } from "../measure";
import { useLabelLayout } from "./labelLayout";
import { labelForms, labelText, partsText } from "./labelText";
import type { Curve, Placement } from "./placeLabels";
import "./lines.css";

/** The label's padding (6px each side) and height (16px text, 2px above and below). */
const PADDING = 12;
export const LABEL_HEIGHT = 20;

export interface LineLabelProps {
  /** The arrow's id. */
  readonly id: string;
  /** The curve the line draws, in canvas units. */
  readonly curve: Curve;
  readonly label: LineLabelModel;
  /** Where the page's label layout puts it (`useLabelPlacement`): null or undefined draws nothing. */
  readonly placement: Placement | null | undefined;
  readonly onSelect: () => void;
}

export function LineLabel({ id, curve, label, placement, onSelect }: LineLabelProps) {
  const layout = useLabelLayout();
  const measure = useMeasure();
  const forms = useMemo(() => labelForms(label), [label]);
  const full = useMemo(() => labelText(label, false), [label]);
  const widths = useMemo(() => forms.map((form) => Math.ceil(measure(partsText(form), LINE_LABEL_FONT)) + PADDING), [forms, measure]);
  useLayoutEffect(() => {
    layout.report(id, { curve, widths, height: LABEL_HEIGHT });
  }, [layout, id, curve, widths]);
  useLayoutEffect(
    () => () => {
      layout.report(id, null);
    },
    [layout, id],
  );
  const parts = placement ? forms[placement.form] : undefined;
  if (!placement || !parts) {
    return null;
  }
  return (
    <button
      type="button"
      className="line-label nodrag nopan"
      style={{ width: widths[placement.form], height: LABEL_HEIGHT, transform: `translate(-50%, -50%) translate(${placement.x}px, ${placement.y}px)` }}
      aria-label={`Line ${full}`}
      title={full}
      data-arrow={id}
      onClick={(event) => {
        event.stopPropagation();
        onSelect();
      }}
    >
      {parts.map((part, index) => (
        <span key={index} className={`line-label-${part.tone}`}>
          {part.text}
        </span>
      ))}
    </button>
  );
}
