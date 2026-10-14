/**
 * The page as the canvas drew it, for the page checks and the image's bounds: each drawn node's box
 * (React Flow's measured size at its absolute position), and each line's path sampled from the SVG the
 * canvas drew (an edge's first path is its line). Lines read nothing where the browser can't measure
 * paths.
 */

import type { ReactFlowState } from "@xyflow/react";
import type { Box, DrawnLine, PageGeometry, Point } from "../board/checks";

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

/** The page as drawn under `container`, from the canvas's state. */
export function drawnGeometry(container: ParentNode, state: Pick<ReactFlowState, "nodeLookup" | "edges">): PageGeometry {
  const edges = byDataId(container, ".react-flow__edge");
  const boxes = new Map<string, Box>();
  for (const node of state.nodeLookup.values()) {
    const { width, height } = node.measured;
    if (node.hidden === true || !width || !height) {
      continue;
    }
    boxes.set(node.id, { x: node.internals.positionAbsolute.x, y: node.internals.positionAbsolute.y, width, height });
  }
  const lines: DrawnLine[] = state.edges
    .filter((edge) => edge.hidden !== true)
    .map((edge) => ({ id: edge.id, from: edge.source, to: edge.target, points: sample(edges.get(edge.id)?.querySelector("path") ?? null) }));
  return { boxes, lines };
}
