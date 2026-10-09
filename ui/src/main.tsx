/** Entry point: apply the theme, learn which window this is, and render the shell. */

import { QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { queryClient } from "./ipc/client";
import { windowCommand } from "./ipc/unfold";
import { Shell } from "./shell/Shell";
import { installTransferReceiver } from "./state/boards/transfer";
import { useWindow } from "./state/window";
import "./theme/theme";

async function start(): Promise<void> {
  // Before anything awaits: a board another window moves here may arrive while this one loads.
  installTransferReceiver();
  // Outside the app there is no main process to ask; the shell then shows the welcome screen.
  const info = await windowCommand("window_info").catch(() => null);
  useWindow.getState().setInfo(info);

  const container = document.getElementById("root");
  if (!container) {
    throw new Error("index.html has no #root element");
  }
  createRoot(container).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <Shell />
      </QueryClientProvider>
    </StrictMode>,
  );
}

void start();
