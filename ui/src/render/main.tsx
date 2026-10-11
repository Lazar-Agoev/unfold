/**
 * The render window's entry (page images and page checks): it draws the pages the main process hands
 * it, read-only, for an image or the page checks, and never shows. It follows engine events so the
 * cards it keeps loaded stay current between pages.
 */

import { QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { installEngineInvalidation, queryClient } from "../ipc";
import "../theme/theme";
import { installRenderJobs } from "./jobs";
import { RenderApp } from "./RenderApp";

// Before anything awaits: the first page may arrive as soon as the window has loaded.
installRenderJobs();
installEngineInvalidation(queryClient);

const container = document.getElementById("root");
if (!container) {
  throw new Error("render.html has no #root element");
}
createRoot(container).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RenderApp />
    </QueryClientProvider>
  </StrictMode>,
);
