/**
 * First launch (05): an empty workspace. Add project… (⌘O) opens the folder picker; a folder dropped
 * on the zone (or anywhere on the window, which the frame handles) is added; "Opened before" lists the
 * folders removed from the workspace, with when (they were open until removed), each added back with
 * `readd_project` (V-21, A-1).
 */

import { useState, type DragEvent } from "react";
import { Button, TextButton, showToast } from "../../components";
import { Icon } from "../../icons";
import { toUnfoldError, type RemovedProject } from "../../ipc";
import { ActionIds, useShortcut } from "../../keyboard";
import { displayPath, relativeTime } from "../../lib/format";
import { addProject } from "../../state/actions";
import { setHomeView } from "../../state/layout";
import { activateHome } from "../../state/tabs";
import { readdProject } from "../../state/workspace";
import "./firstLaunch.css";

async function addBack(folder: RemovedProject): Promise<void> {
  try {
    await readdProject(folder.path);
    setHomeView("overview");
    activateHome();
  } catch (error) {
    showToast({ title: `Couldn't add ${folder.name}`, body: toUnfoldError(error).message, tone: "danger" });
  }
}

function carriesFiles(event: DragEvent): boolean {
  return event.dataTransfer.types.includes("Files");
}

/** The drop zone: its look follows a drag over it; the drop itself goes on to the window's handler. */
function DropZone() {
  const [over, setOver] = useState(0);
  return (
    <div
      className={over > 0 ? "home-drop home-drop-over" : "home-drop"}
      onDragEnter={(event) => {
        if (carriesFiles(event)) {
          setOver((count) => count + 1);
        }
      }}
      onDragLeave={(event) => {
        if (carriesFiles(event)) {
          setOver((count) => Math.max(0, count - 1));
        }
      }}
      onDrop={() => {
        setOver(0);
      }}
    >
      {/* The set's folder at 24px with a hairline stroke, as drawn (the icon set stops at 16px). */}
      <svg className="home-drop-icon" width="24" height="24" viewBox="0 0 16 16" aria-hidden="true">
        <path d="M2 4.5C2 3.67 2.67 3 3.5 3H6.5L8 4.5H12.5C13.33 4.5 14 5.17 14 6V11.5C14 12.33 13.33 13 12.5 13H3.5C2.67 13 2 12.33 2 11.5V4.5Z" />
      </svg>
      <span className="home-drop-words">
        <span className="home-drop-title">Drop a folder here</span>
        <span className="home-drop-hint">A repository with TypeScript or Python files</span>
      </span>
    </div>
  );
}

export function FirstLaunch({ removed }: { readonly removed: readonly RemovedProject[] }) {
  const shortcut = useShortcut(ActionIds.addProject) ?? undefined;
  return (
    <div className="home-first">
      <div className="home-first-block">
        <div className="home-first-intro">
          <p className="home-caps">Unfold</p>
          <h1 className="home-first-headline">Read a codebase by how it starts.</h1>
          <p className="home-first-body">
            Connect a repository. Unfold reads TypeScript and Python, finds the ways the code starts, and lets you follow each call on a
            board. It never runs your code.
          </p>
        </div>
        <div className="home-first-actions">
          <Button variant="primary" {...(shortcut === undefined ? {} : { shortcut })} onClick={() => void addProject()}>
            Add project…
          </Button>
        </div>
        <div className="home-first-drop">
          <DropZone />
        </div>
        {removed.length > 0 ? (
          <div className="home-first-before">
            <h2 className="home-caps home-first-before-label">Opened before</h2>
            <ul className="home-first-before-list">
              {removed.map((folder) => (
                <li key={folder.path} className="home-first-before-row">
                  <Icon name="folder" className="home-first-before-icon" />
                  {/* The folder's name tells the rows apart, so a long path loses its start. */}
                  <span className="home-first-before-path" title={folder.path}>
                    <bdi>{displayPath(folder.path)}</bdi>
                  </span>
                  <span className="home-first-before-when">{`last opened ${relativeTime(folder.removedAtMs)}`}</span>
                  <span className="home-first-before-action">
                    <TextButton aria-label={`Add ${folder.name}`} onClick={() => void addBack(folder)}>
                      Add
                    </TextButton>
                  </span>
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </div>
    </div>
  );
}
