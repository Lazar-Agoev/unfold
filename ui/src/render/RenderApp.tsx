/**
 * The render window's content: the page it was asked to draw, in a canvas of its own (each page starts
 * from its own camera).
 */

import { ReactFlowProvider } from "@xyflow/react";
import { useRenderJob } from "./jobs";
import { RenderPage } from "./RenderPage";
import "./render.css";

export function RenderApp() {
  const job = useRenderJob((state) => state.job);
  return job ? (
    <ReactFlowProvider key={job.requestId}>
      <RenderPage job={job} />
    </ReactFlowProvider>
  ) : null;
}
