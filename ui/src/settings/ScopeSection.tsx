/**
 * Project settings → Scope (08, V-70): a check-box tree of the project's folders, one `scope_dir`
 * level per expanded folder, with the folders discovery skips listed last with their reason, and the
 * Rules the boxes write. Every change is staged; the dialog saves them together.
 */

import { useQueries } from "@tanstack/react-query";
import { useCallback, useMemo, useState } from "react";
import { Checkbox, IconButton, TextButton, TreeView, type TreeChildren, type TreeRowContent } from "../components";
import { Icon } from "../icons";
import { engineQueryOptions, type ProjectId, type ScopeFolder } from "../ipc";
import { formatCount } from "../lib/format";
import {
  canToggle,
  checkOf,
  folderScope,
  ruleRows,
  skippedByDiscovery,
  toggleFolder,
  toggleRule,
  treeOrder,
  type FolderScope,
  type ScopeLists,
} from "./scope";

interface ScopeSectionProps {
  readonly project: ProjectId;
  readonly name: string;
  /** The lists `scope_dir` reflects (the file when the dialog opened). */
  readonly saved: ScopeLists;
  readonly draft: ScopeLists;
  readonly onChange: (next: ScopeLists) => void;
}

const ROOT = "";
const folderId = (folder: ScopeFolder) => folder.path;
const folderName = (folder: ScopeFolder) => folder.name;
const canExpand = (folder: ScopeFolder) => !skippedByDiscovery(folder);

/** A folder's reason column: why it isn't graphed, with what is staged. */
function reasonOf(scope: FolderScope): string | null {
  switch (scope.kind) {
    case "fixed":
      return scope.reason;
    case "excluded":
      return "excluded";
    case "not_included":
      return "not included";
    default:
      return null;
  }
}

function ScopeTree({ project, name, saved, draft, onChange }: ScopeSectionProps) {
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const paths = useMemo(() => [ROOT, ...expanded], [expanded]);
  const levels = useQueries({ queries: paths.map((path) => engineQueryOptions("scope_dir", { project, path })) });
  const root = levels[0];
  const roots = useMemo(() => treeOrder(root?.data?.folders ?? []), [root?.data]);

  // One entry per expanded level; changes only when a level's state does.
  const childrenKey = levels.map((level) => `${level.status}:${level.dataUpdatedAt}`).join(",");
  const children = useMemo(() => {
    const map = new Map<string, TreeChildren<ScopeFolder>>();
    paths.forEach((path, index) => {
      const level = levels[index];
      if (level?.data) {
        map.set(path, treeOrder(level.data.folders));
      } else if (level?.error) {
        map.set(path, { status: "error", message: level.error.message });
      } else {
        map.set(path, { status: "loading" });
      }
    });
    return map;
    // `childrenKey` stands for the levels' states.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [paths, childrenKey]);
  const getChildren = useCallback((folder: ScopeFolder) => children.get(folder.path), [children]);

  const toggle = (folder: ScopeFolder) => {
    onChange(toggleFolder(folder, saved, draft));
  };

  const renderRow = (folder: ScopeFolder): TreeRowContent => {
    const scope = folderScope(folder, saved, draft);
    const fixed = scope.kind === "fixed";
    const reason = reasonOf(scope);
    return {
      label: (
        <span className="scope-row">
          {/* The row toggles on click and Space; the box takes its own clicks without the row's. */}
          <span
            className="scope-row-box"
            onClick={(event) => {
              event.stopPropagation();
            }}
          >
            <Checkbox
              tabIndex={-1}
              label={`Graph ${folder.path}`}
              checked={checkOf(scope)}
              disabled={!canToggle(scope)}
              onChange={() => {
                toggle(folder);
              }}
            />
          </span>
          <Icon name="folder" className={fixed ? "scope-row-folder scope-row-folder-faint" : "scope-row-folder"} />
          <span className="scope-row-name">{folder.name}</span>
        </span>
      ),
      marks: (
        <>
          {reason === null ? null : <span className="scope-row-reason">{reason}</span>}
          <span className="scope-row-count">{fixed ? "" : formatCount(folder.sources)}</span>
        </>
      ),
      dimmed: fixed,
      ...(scope.kind === "excluded" && scope.locked !== null ? { title: `Excluded with ${scope.locked}. Remove that rule to include it.` } : {}),
    };
  };

  return (
    <div className="scope-tree">
      <div className="scope-tree-header" aria-hidden="true">
        <span className="scope-tree-heading">Folders in {name}</span>
        <span className="scope-tree-files">Files</span>
      </div>
      {root?.error ? (
        <p className="scope-tree-message scope-tree-error">{root.error.message}</p>
      ) : !root?.data ? (
        <p className="scope-tree-message">Loading folders…</p>
      ) : roots.length === 0 ? (
        <p className="scope-tree-message">This project has no folders.</p>
      ) : (
        <TreeView<ScopeFolder>
          label={`Folders in ${name}`}
          className="scope-tree-body"
          roots={roots}
          getId={folderId}
          getText={folderName}
          isExpandable={canExpand}
          getChildren={getChildren}
          renderRow={renderRow}
          expanded={expanded}
          onExpandedChange={setExpanded}
          toggleOnClick={false}
          onSelect={toggle}
        />
      )}
      {root?.data?.truncated ? <p className="scope-tree-message">Only the first 2,000 folders are listed.</p> : null}
    </div>
  );
}

function Rules({ saved, draft, onChange }: Pick<ScopeSectionProps, "saved" | "draft" | "onChange">) {
  const rows = ruleRows(saved, draft);
  return (
    <section className="scope-rules" aria-labelledby="scope-rules-title">
      <div className="scope-rules-heading">
        <h4 id="scope-rules-title" className="settings-caps">
          Rules
        </h4>
        <span className="settings-hint">Written to the file from the boxes above.</span>
      </div>
      {rows.length === 0 ? (
        <p className="scope-rules-empty">None: every folder is graphed except the ones skipped above.</p>
      ) : (
        <ul className="scope-rules-list">
          {rows.map((row) => (
            <li key={`${row.list}:${row.pattern}`} className="scope-rule" data-status={row.status}>
              <span className="scope-rule-kind">{row.list}</span>
              <span className="scope-rule-pattern">{row.pattern}</span>
              {row.status === "removed" ? (
                <TextButton
                  tone="muted"
                  onClick={() => {
                    onChange(toggleRule(draft, row));
                  }}
                >
                  Keep
                </TextButton>
              ) : (
                <IconButton
                  icon="close"
                  label={`Remove ${row.list} ${row.pattern}`}
                  onClick={() => {
                    onChange(toggleRule(draft, row));
                  }}
                />
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

export function ScopeSection(props: ScopeSectionProps) {
  return (
    <>
      <ScopeTree {...props} />
      <Rules saved={props.saved} draft={props.draft} onChange={props.onChange} />
    </>
  );
}
