/**
 * The render window's content: the page it was asked to draw, in the person's theme, in a canvas of
 * its own (each page starts from its own camera).
 */

import { ReactFlowProvider } from "@xyflow/react";
import { usePreferencesTheme } from "../preferences";
import { useRenderJob } from "./jobs";
import { RenderPage } from "./RenderPage";
import "./render.css";

export function RenderApp() {
  usePreferencesTheme();
  const job = useRenderJob((state) => state.job);
  return job ? (
    <ReactFlowProvider key={job.requestId}>
      <RenderPage job={job} />
    </ReactFlowProvider>
  ) : null;
}
