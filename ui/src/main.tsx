/** Entry point: apply the theme, learn which window this is, and render the window. */

import { QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app";
import { queryClient, windowCommand } from "./ipc";
import { installTransferReceiver } from "./state/boards";
import { setWindowInfo } from "./state/window";
import "./theme/theme";

async function start(): Promise<void> {
  // Before anything awaits: a board another window moves here may arrive while this one loads.
  installTransferReceiver();
  // Outside the app there is no main process to ask.
  setWindowInfo(await windowCommand("window_info").catch(() => null));

  const container = document.getElementById("root");
  if (!container) {
    throw new Error("index.html has no #root element");
  }
  createRoot(container).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <App />
      </QueryClientProvider>
    </StrictMode>,
  );
}

void start();
