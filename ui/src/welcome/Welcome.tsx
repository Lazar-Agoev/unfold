/**
 * The welcome screen, shown while the window has no folder (architecture §11.6): Open Folder (⌘O),
 * the recent folders with when they were last opened, and dropping a folder anywhere to open it.
 */

import { useQueryClient } from "@tanstack/react-query";
import { FolderOpen, X } from "lucide-react";
import { useEffect, useState, type DragEvent } from "react";
import { Button, IconButton } from "../components";
import { call } from "../ipc/commands";
import type { RecentFolder } from "../ipc/generated";
import { engineKey, useEngineQuery } from "../ipc/queries";
import { ActionIds, actions, useShortcut } from "../keyboard";
import { displayPath } from "../state/window";
import { openDroppedFolder, openRecentFolder } from "./openFolder";
import { relativeTime } from "./time";
import "./Welcome.css";

function hasFiles(event: DragEvent): boolean {
  return [...event.dataTransfer.types].includes("Files");
}

function RecentRow({ folder, onForget }: { readonly folder: RecentFolder; readonly onForget: () => void }) {
  return (
    <li className="welcome-recent-row">
      <button
        type="button"
        className="welcome-recent-open"
        title={folder.path}
        onClick={() => void openRecentFolder(folder.path)}
      >
        <span className="welcome-recent-name">{folder.name}</span>
        <span className="welcome-recent-path">{displayPath(folder.path)}</span>
        <span className="welcome-recent-time">{relativeTime(folder.openedAtMs)}</span>
      </button>
      <IconButton icon={X} label={`Remove ${folder.name} from Recent`} size="small" className="welcome-recent-forget" onClick={onForget} />
    </li>
  );
}

export function Welcome() {
  const shortcut = useShortcut(ActionIds.openFolder) ?? undefined;
  const client = useQueryClient();
  const recent = useEngineQuery("recent_folders", {});
  // The list outlives folder changes (it is app-wide), but a folder opened since it was read — the
  // one Close Folder just left, or one opened in another window — must show: re-read it each time.
  useEffect(() => {
    void client.invalidateQueries({ queryKey: engineKey("recent_folders", {}) });
  }, [client]);
  const [dragging, setDragging] = useState(false);

  const forget = async (path: string) => {
    const next = await call("forget_recent_folder", { path });
    client.setQueryData(engineKey("recent_folders", {}), next);
  };

  const folders = recent.data?.folders ?? [];

  return (
    <main
      className="welcome"
      data-dragging={dragging || undefined}
      onDragOver={(event) => {
        if (hasFiles(event)) {
          event.preventDefault();
          event.dataTransfer.dropEffect = "copy";
          setDragging(true);
        }
      }}
      onDragLeave={(event) => {
        if (event.currentTarget === event.target) {
          setDragging(false);
        }
      }}
      onDrop={(event) => {
        setDragging(false);
        const file = event.dataTransfer.files[0];
        if (file) {
          event.preventDefault();
          void openDroppedFolder(file);
        }
      }}
    >
      <div className="welcome-inner">
        <h1 className="welcome-wordmark">unfold</h1>
        <p className="welcome-tagline">See how a codebase starts, and follow where it goes.</p>
        <Button
          variant="primary"
          size="large"
          icon={FolderOpen}
          className="welcome-open"
          {...(shortcut ? { shortcut } : {})}
          onClick={() => {
            actions.run(ActionIds.openFolder);
          }}
        >
          Open Folder…
        </Button>
        <p className="welcome-drop">{dragging ? "Drop the folder to open it" : "or drop a folder here"}</p>
        {folders.length > 0 ? (
          <section className="welcome-recent" aria-labelledby="welcome-recent-title">
            <h2 id="welcome-recent-title" className="welcome-recent-title">
              Recent
            </h2>
            <ul className="welcome-recent-list">
              {folders.map((folder) => (
                <RecentRow key={folder.path} folder={folder} onForget={() => void forget(folder.path)} />
              ))}
            </ul>
          </section>
        ) : null}
      </div>
    </main>
  );
}
