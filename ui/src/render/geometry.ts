/**
 * The page as the canvas drew it, for the page checks and the image's bounds: each drawn node's box
 * (React Flow's measured size at its absolute position), each line's path sampled from the SVG the
 * canvas drew (an edge's first path is its line), and each card drawn in Flow with its visible steps
 * and the steps that still show a name from the code (counted as the page's outline counts them, so
 * they are the steps `label_steps` can label). Lines read nothing where the browser can't measure paths.
 */

import type { ReactFlowState } from "@xyflow/react";
import type { Box, DrawnFlow, DrawnLine, PageGeometry, Point } from "../board/checks";
import { isWithin } from "../code/outline";
import type { Card, CardView, FlowNodeKind, Page } from "../ipc/generated";
import { labelTexts } from "../state/boards";

/** One sample of a line's path per this many canvas units. */
const SAMPLE_EVERY = 12;
/** The most samples one line takes. */
const MAX_SAMPLES = 200;

/** The elements under `container` matching `selector`, by their `data-id`. */
function byDataId(container: ParentNode, selector: string): Map<string, Element> {
  return new Map([...container.querySelectorAll(selector)].flatMap((element) => {
    const id = element.getAttribute("data-id");
    return id === null ? [] : [[id, element] as const];
  }));
}

/** The node ids the canvas has drawn (a page of many cards draws only those in view). */
export function drawnNodeIds(container: ParentNode): ReadonlySet<string> {
  return new Set(byDataId(container, ".react-flow__node").keys());
}

/** Points along a path, in its own (canvas) coordinates; none when paths can't be measured. */
function sample(path: SVGPathElement | null): Point[] {
  if (!path || typeof path.getTotalLength !== "function") {
    return [];
  }
  const length = path.getTotalLength();
  const steps = Math.min(MAX_SAMPLES, Math.max(2, Math.ceil(length / SAMPLE_EVERY)));
  return Array.from({ length: steps + 1 }, (_, index) => {
    const point = path.getPointAtLength((length * index) / steps);
    return { x: point.x, y: point.y };
  });
}

/**
 * How many of a card's Flow steps have no label on the page: the steps of its Flow model with a key
 * (start and end aside, and those hidden in a collapsed step group of the card: a member's key or a key
 * under it), as the page's outline counts them; none while its source isn't current.
 */
export function unlabeledSteps(page: Page, card: Card, model: CardView | undefined): number {
  if (model?.type !== "block" || model.flow?.source !== "current") {
    return 0;
  }
  const labels = labelTexts(page.labels);
  const hidden = page.groups.filter((group) => group.card === card.id && group.collapsed).flatMap((group) => group.members);
  const counts = (key: string | undefined, kind: FlowNodeKind) =>
    key !== undefined && kind !== "start" && kind !== "end" && labels[key] === undefined && !hidden.some((member) => isWithin(key, member));
  return model.flow.nodes.filter((node) => counts(node.key, node.kind)).length;
}

/**
 * The page as drawn under `container`, from the canvas's state; `unlabeledOf` counts a Flow card's
 * steps without a label.
 */
export function drawnGeometry(container: ParentNode, state: Pick<ReactFlowState, "nodeLookup" | "edges">, unlabeledOf: (card: string) => number): PageGeometry {
  const nodes = byDataId(container, ".react-flow__node");
  const edges = byDataId(container, ".react-flow__edge");
  const boxes = new Map<string, Box>();
  const flows = new Map<string, DrawnFlow>();
  for (const node of state.nodeLookup.values()) {
    const { width, height } = node.measured;
    if (node.hidden === true || !width || !height) {
      continue;
    }
    boxes.set(node.id, { x: node.internals.positionAbsolute.x, y: node.internals.positionAbsolute.y, width, height });
    // A card drawn in Flow: its own steps as drawn (start and end aside; a call expanded in place shows
    // its callee's steps, not the card's).
    const element = nodes.get(node.id);
    if (element?.querySelector(".flow-view")) {
      const steps = [...element.querySelectorAll("[data-flow-node]:not(.flow-pill)")].filter((step) => step.closest(".flow-inset") === null).length;
      flows.set(node.id, { steps, unlabeled: unlabeledOf(node.id) });
    }
  }
  const lines: DrawnLine[] = state.edges
    .filter((edge) => edge.hidden !== true)
    .map((edge) => ({ id: edge.id, from: edge.source, to: edge.target, points: sample(edges.get(edge.id)?.querySelector("path") ?? null) }));
  return { boxes, lines, flows };
}
