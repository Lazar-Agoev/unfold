/** One arrow between cards: right from its port, orthogonal, into the target's header. Memoized. */

import { BaseEdge, getSmoothStepPath, type Edge, type EdgeProps } from "@xyflow/react";
import { memo } from "react";

export interface ArrowData extends Record<string, unknown> {
  readonly dashed: boolean;
  readonly label: string;
}

export type ArrowEdgeType = Edge<ArrowData, "arrow">;

export const ArrowEdge = memo(function ArrowEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  data,
  markerEnd,
}: EdgeProps<ArrowEdgeType>) {
  const [path] = getSmoothStepPath({ sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition, borderRadius: 10, offset: 20 });
  return <BaseEdge id={id} path={path} {...(markerEnd !== undefined ? { markerEnd } : {})} className={data?.dashed ? "board-arrow dashed" : "board-arrow"} />;
});
