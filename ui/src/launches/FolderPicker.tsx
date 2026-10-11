/**
 * "Runs in": the launch's working folder, chosen from the project's folders. The control opens a
 * list under itself (one `scope_dir` level at a time) where a folder is chosen, or opened to choose
 * one inside it. Folders discovery always skips (`node_modules`, `.git`) aren't offered.
 */

import { useId, useState } from "react";
import { IconButton, TextButton } from "../components";
import { useEngineQuery, type ProjectId } from "../ipc";
import { Icon } from "../icons";

interface FolderPickerProps {
  readonly project: ProjectId;
  /** Shown for the project's folder (`.`). */
  readonly projectName: string;
  /** The project-relative folder, `.` for the project's folder. */
  readonly value: string;
  readonly onChange: (folder: string) => void;
  /** The field label's id. */
  readonly labelledBy: string;
}

/** The folder that holds `path` (`""` for the project's folder). */
function parentOf(path: string): string {
  const cut = path.lastIndexOf("/");
  return cut < 0 ? "" : path.slice(0, cut);
}

function FolderList({ project, projectName, start, onChoose }: { readonly project: ProjectId; readonly projectName: string; readonly start: string; readonly onChoose: (folder: string) => void }) {
  const [path, setPath] = useState(start);
  const level = useEngineQuery("scope_dir", { project, path });
  const folders = (level.data?.folders ?? []).filter((folder) => folder.skipped !== "default" && folder.skipped !== "symlink" && folder.skipped !== "invalid_path");
  return (
    <div className="launch-folders">
      <div className="launch-folders-head">
        {path === "" ? null : (
          <IconButton
            icon="back"
            size={20}
            label="Up one folder"
            onClick={() => {
              setPath(parentOf(path));
            }}
          />
        )}
        <span className="launch-folders-path">{path === "" ? projectName : path}</span>
        <TextButton
          onClick={() => {
            onChoose(path === "" ? "." : path);
          }}
        >
          Use this folder
        </TextButton>
      </div>
      <ul className="launch-folders-list" aria-label={`Folders in ${path === "" ? projectName : path}`}>
        {level.isPending ? <li className="launch-folders-note">Loading…</li> : null}
        {level.data && folders.length === 0 ? <li className="launch-folders-note">No folders inside.</li> : null}
        {folders.map((folder) => (
          <li key={folder.path} className="launch-folders-row">
            <button
              type="button"
              className="launch-folders-choose"
              onClick={() => {
                onChoose(folder.path);
              }}
            >
              <Icon name="folder" size={14} />
              <span>{folder.name}</span>
            </button>
            <IconButton
              icon="chevron-right"
              size={20}
              label={`Folders in ${folder.name}`}
              onClick={() => {
                setPath(folder.path);
              }}
            />
          </li>
        ))}
      </ul>
    </div>
  );
}

export function FolderPicker({ project, projectName, value, onChange, labelledBy }: FolderPickerProps) {
  const [open, setOpen] = useState(false);
  const listId = useId();
  const valueId = useId();
  const current = value === "." ? "" : value;
  return (
    <>
      <button
        type="button"
        className="launch-select launch-folder"
        aria-labelledby={`${labelledBy} ${valueId}`}
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        onClick={() => {
          setOpen(!open);
        }}
      >
        <Icon name="folder" />
        <span id={valueId} className="launch-select-value launch-mono">{current === "" ? projectName : current}</span>
        <Icon name="chevron-down-small" size={10} />
      </button>
      {open ? (
        <div id={listId}>
          <FolderList
            project={project}
            projectName={projectName}
            start={parentOf(current)}
            onChoose={(folder) => {
              onChange(folder);
              setOpen(false);
            }}
          />
        </div>
      ) : null}
    </>
  );
}
