/**
 * The rows of Add's lists (04 §5.1, 04b §6): a result of any kind (project monogram, kind column, name,
 * trailing meta), the "N more <kind>" link, and a command (icon, group ›, name with its match in bold,
 * keycaps).
 */

import { KeyChip, Monogram } from "../components";
import { Icon } from "../icons";
import type { CommandItem } from "./commands";
import { highlightRanges } from "./highlight";
import { baseName, definitionColumn, entranceColumn, trimmedMatch, type ResultItem } from "./search";

/** Row heights: results and commands 32, the "more" link 28 (04 §5.1). */
export const ROW_HEIGHT = 32;
const MORE_HEIGHT = 28;

export function resultHeight(item: ResultItem): number {
  return item.type === "more" ? MORE_HEIGHT : ROW_HEIGHT;
}

/** A result's accessible name. */
export function resultText(item: ResultItem): string {
  switch (item.type) {
    case "entrance":
      return `Entrance ${entranceColumn(item.hit)} ${item.hit.name}`;
    case "function":
      return `Function ${item.hit.qualifiedName}, ${item.hit.path}`;
    case "file":
      return `File ${item.hit.path}`;
    case "text":
      return `${item.hit.file.path} line ${item.hit.match.line}: ${item.hit.match.text.trim()}`;
    case "more":
      return item.label;
  }
}

interface ResultRowProps {
  readonly item: Exclude<ResultItem, { type: "more" }>;
  readonly selected: boolean;
  readonly projectName: string;
  /** Why it can't be added here (a same-name project, V-62), if it can't. */
  readonly blocked: string | null;
}

function Meta({ item, projectName }: { readonly item: ResultRowProps["item"]; readonly projectName: string }) {
  switch (item.type) {
    case "entrance":
      return <span className="add-row-meta">{`${projectName} · ${baseName(item.hit.path)}`}</span>;
    case "function":
      return <span className="add-row-meta add-code">{item.hit.path}</span>;
    case "file":
      return null;
    case "text":
      return <span className="add-row-meta add-code">{`${item.hit.file.path}:${item.hit.match.line}`}</span>;
  }
}

function Name({ item }: { readonly item: ResultRowProps["item"] }) {
  switch (item.type) {
    case "entrance":
      return <span className="add-row-name">{item.hit.name}</span>;
    case "function":
      return <span className="add-row-name">{item.hit.qualifiedName}</span>;
    case "file":
      return <span className="add-row-name">{item.hit.path}</span>;
    case "text": {
      const { text, ranges } = trimmedMatch(item.hit.match);
      return (
        <span className="add-row-name snippet">
          {item.hit.match.column > 0 ? "…" : ""}
          {highlightRanges(text, ranges, "add-text-match")}
        </span>
      );
    }
  }
}

function kindColumn(item: ResultRowProps["item"]): string {
  switch (item.type) {
    case "entrance":
      return entranceColumn(item.hit);
    case "function":
      return definitionColumn(item.hit.kind);
    case "file":
      return "FILE";
    case "text":
      return "TEXT";
  }
}

export function ResultRow({ item, selected, projectName, blocked }: ResultRowProps) {
  return (
    <div className={blocked === null ? "add-row" : "add-row blocked"} title={blocked ?? undefined}>
      <Monogram name={projectName} size={20} tone={selected ? "selected" : "default"} />
      <span className="add-row-kind">{kindColumn(item)}</span>
      <Name item={item} />
      {blocked === null ? <Meta item={item} projectName={projectName} /> : <span className="add-row-meta">{blocked}</span>}
    </div>
  );
}

export function MoreRow({ label }: { readonly label: string }) {
  return <div className="add-more">{label}</div>;
}

export function CommandRow({ item, selected, shortcut }: { readonly item: CommandItem; readonly selected: boolean; readonly shortcut: string | null }) {
  const { action, prefix, ranges } = item;
  return (
    <div className="add-command">
      <span className="add-command-icon">{action.icon ? <Icon name={action.icon} /> : null}</span>
      <span className="add-command-name">
        {prefix === null ? null : (
          <>
            <span className="add-command-group">{prefix}</span>
            <span className="add-command-group">›</span>
          </>
        )}
        <span className="add-command-title">{highlightRanges(action.title, ranges, "add-command-match")}</span>
      </span>
      {shortcut === null ? null : <KeyChip variant="keycap" shortcut={shortcut} selected={selected} />}
    </div>
  );
}
