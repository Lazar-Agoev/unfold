/**
 * The window: title bar, then the welcome screen (no folder), the first-Compile screen (a folder
 * never compiled), or the workbench; then the status bar.
 */

import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { forgetVerifiedCode } from "../canvas/cardData";
import { showToast, Toaster } from "../components";
import { useConnectionState } from "../ipc/hooks";
import { clearFolderQueries, installEngineInvalidation } from "../ipc/queries";
import { onEvent } from "../ipc/unfold";
import { setActionErrorHandler, useMenuCommands, usePlatform, useShortcutDispatcher } from "../keyboard";
import { CommandPalette } from "../palette";
import { PreferencesDialog, usePreferencesKeyBindings, usePreferencesTheme } from "../preferences/PreferencesDialog";
import { SettingsDialogs } from "../settings";
import { forgetAllBoards } from "../state/boards/store";
import { setTransfersReady } from "../state/boards/transfer";
import { followWindowState, restoreWindow } from "../state/persistence";
import { folderName, useWindow } from "../state/window";
import { FirstCompile } from "../welcome/FirstCompile";
import { openDroppedFolder } from "../welcome/openFolder";
import { Welcome } from "../welcome/Welcome";
import "./Shell.css";
import { StatusBar } from "./StatusBar";
import { TitleBar } from "./TitleBar";
import { useCompile } from "./useCompile";
import { useShellActions } from "./useShellActions";
import { Workbench } from "./Workbench";

/** Window-wide wiring: keys, menu commands, action failures, engine events, and folder changes. */
function useWindowWiring(): void {
  const client = useQueryClient();
  useShortcutDispatcher();
  useMenuCommands();
  usePreferencesTheme();
  usePreferencesKeyBindings();

  useEffect(() => installEngineInvalidation(client), [client]);

  useEffect(() => {
    setActionErrorHandler((error, action) => {
      const message = error instanceof Error ? error.message : String(error);
      showToast({ message: `${action.title.replace(/…$/, "")} failed: ${message}`, tone: "danger" });
    });
    return () => {
      setActionErrorHandler(null);
    };
  }, []);

  // The main process reports folder changes it makes itself (Open Folder from the menu, a drop).
  useEffect(
    () =>
      onEvent("folder_changed", (event) => {
        const next = event["folder"];
        const key = event["windowKey"];
        useWindow.getState().setFolder(typeof next === "string" ? next : null, typeof key === "string" ? key : undefined);
      }),
    [],
  );
}

/**
 * Folder-dependent state goes when the folder changes (cached engine data, the old folder's boards,
 * and remembered card renders); the window's saved state comes back.
 */
function useFolderSession(folder: string | null, key: string | null): void {
  const client = useQueryClient();
  const connection = useConnectionState();
  const restored = useRef<string | null>(null);
  const previousFolder = useRef(folder);

  useEffect(() => {
    if (previousFolder.current !== folder) {
      previousFolder.current = folder;
      clearFolderQueries(client);
      // Their edits were written before the switch (welcome/openFolder); the engine no longer has them.
      forgetAllBoards();
      forgetVerifiedCode();
    }
  }, [client, folder]);

  const session = folder !== null && key !== null ? `${folder}\u0000${key}` : null;
  const connected = connection === "connected";
  const [ready, setReady] = useState<string | null>(null);

  // Restore once per folder and key, as soon as the engine can answer; reconnecting keeps the tabs.
  useEffect(() => {
    if (session === null || key === null || !connected || restored.current === session) {
      return;
    }
    restored.current = session;
    // Boards moved here from another window wait until the restore has set up this window's tabs.
    setTransfersReady(false);
    void restoreWindow(key).finally(() => {
      if (restored.current === session) {
        setReady(session);
        setTransfersReady(true);
      }
    });
  }, [session, key, connected]);

  // Then every change is saved.
  useEffect(() => (ready !== null && ready === session && key !== null ? followWindowState(key) : undefined), [ready, session, key]);
}

export function Shell() {
  const folder = useWindow((state) => state.info?.folder ?? null);
  const key = useWindow((state) => state.info?.key ?? null);
  const inApp = useWindow((state) => state.info !== null);
  const platform = usePlatform();
  const compile = useCompile(folder);
  useWindowWiring();
  useShellActions(compile);
  useFolderSession(folder, key);

  useEffect(() => {
    document.title = folder === null ? "Unfold" : `${folderName(folder)} — Unfold`;
  }, [folder]);

  let body;
  if (folder === null) {
    body = <Welcome />;
  } else if (compile.status === null) {
    body = <main className="shell-loading" aria-busy="true" />;
  } else if (compile.status.state.kind === "not_compiled") {
    body = <FirstCompile />;
  } else {
    body = <Workbench />;
  }

  return (
    <div
      className="shell"
      data-platform={inApp ? platform : "browser"}
      // A folder dropped on a window that shows one opens here too, after its boards are saved (the
      // welcome screen handles its own drops).
      onDragOver={(event) => {
        if (folder !== null && !event.defaultPrevented && event.dataTransfer.types.includes("Files")) {
          event.preventDefault();
        }
      }}
      onDrop={(event) => {
        const file = event.dataTransfer.files[0];
        if (folder !== null && !event.defaultPrevented && file) {
          event.preventDefault();
          void openDroppedFolder(file);
        }
      }}
    >
      <TitleBar folder={folder} compile={compile} />
      {body}
      <StatusBar compile={compile} folder={folder} />
      <CommandPalette />
      <PreferencesDialog />
      <SettingsDialogs />
      <Toaster />
    </div>
  );
}
