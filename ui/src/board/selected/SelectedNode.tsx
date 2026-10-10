/**
 * Selected for a Flow node (redesign §4.4, V-60): its label (the page's, for the card's project), the
 * name from the code it replaces, its type and lines, and where its paths go (each selects that node).
 */

import { nodeTypeName } from "../../flow/layout";
import type { Card, FlowModel, FlowNode, ProjectId, RouteKind } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import { cardKey, findPage, itemsFor, keyDisplayName, labelNode, labelsFor, select, useOpenBoard, type PageTarget } from "../../state/boards";
import { NODE_LABEL_FIELD } from "../unlabeled";
import { AuthoredField } from "./AuthoredField";
import { RelationRow } from "./Relations";

const PATH_WORDS: Partial<Record<RouteKind, string>> = {
  yes: "Yes",
  no: "No",
  elseIf: "No (else if)",
  each: "Each",
  done: "Done",
  error: "On error",
  return: "Returns",
};

/** The node a Flow model shows for `key`, if any (End has no key). */
export function flowNodeFor(model: FlowModel | null | undefined, key: string): FlowNode | undefined {
  return model?.nodes.find((node) => node.key === key);
}

export interface SelectedNodeProps {
  readonly target: PageTarget;
  readonly card: Card;
  readonly project: ProjectId;
  readonly node: FlowNode;
  /** The card's Flow model without the page's labels: its fallback names. */
  readonly model: FlowModel;
}

export function SelectedNode({ target, card, project, node, model }: SelectedNodeProps) {
  const key = node.key ?? "";
  const author = useAuthor();
  const allLabels = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.labels);
  const health = useOpenBoard(target.board, (board) => board.health);
  const labels = labelsFor(allLabels ?? [], card.ref.project);
  const label = (allLabels ?? []).find((entry) => entry.project === card.ref.project && entry.key === key);
  const issues = itemsFor(health, target.page, { label: { key, ...(card.ref.project !== undefined ? { project: card.ref.project } : {}) } });
  const held = issues.some((item) => item.kind === "review");
  const shownLabel = (other: FlowNode) => (other.key !== undefined ? (labels[other.key] ?? other.label) : other.label);
  const lines =
    node.startLine !== undefined ? (node.endLine !== undefined && node.endLine !== node.startLine ? `lines ${node.startLine}–${node.endLine}` : `line ${node.startLine}`) : "";
  const paths = model.routes
    .filter((route) => route.from === node.id && route.kind !== "sequence")
    .flatMap((route) => {
      const to = model.nodes[route.to];
      return to ? [{ route, to }] : [];
    });
  const next = model.routes.find((route) => route.from === node.id && route.kind === "sequence");
  const nextNode = next && paths.length === 0 ? model.nodes[next.to] : undefined;
  const fallback = node.fallback === true ? node.label : null;
  const go = (to: FlowNode) => () => {
    if (to.key !== undefined) {
      select(target, { kind: "card", cardId: card.id, key: to.key });
    }
  };
  return (
    <>
      <div className="selected-identity">
        <h3 className="selected-name ui">{shownLabel(node)}</h3>
        <p className="selected-location">
          {nodeTypeName(node)}
          {lines ? ` · ${lines}` : ""} · in{" "}
          <button
            type="button"
            className="selected-link"
            onClick={() => {
              select(target, { kind: "card", cardId: card.id, key: null });
            }}
          >
            {keyDisplayName(cardKey(card))}
          </button>
        </p>
      </div>
      <AuthoredField
        label="Flow label"
        value={label}
        placeholder={fallback ?? "A plain-language label…"}
        focusKey={NODE_LABEL_FIELD}
        disabledReason={held ? "Held for review: accept or reconnect it on the card first." : author === null ? "Loading who you are…" : null}
        status={
          <>
            {fallback !== null ? (
              <span className="authored-note">
                Replaces the name from the code: <code>{fallback}</code>
              </span>
            ) : null}
            {issues.map((item) => (
              <span key={item.message} className="authored-note">
                {item.message}
              </span>
            ))}
          </>
        }
        onChange={(text) => {
          if (author !== null && key.length > 0) {
            labelNode(target, project, key, text, author);
          }
        }}
      />
      {paths.length > 0 || nextNode ? (
        <div className="selected-relations">
          {paths.map(({ route, to }, index) => (
            <RelationRow key={`${route.kind}:${route.to}`} {...(index === 0 ? { label: "Paths" } : {})} onActivate={go(to)}>
              <span className="relation-path">{PATH_WORDS[route.kind] ?? route.kind}</span> {shownLabel(to)}
            </RelationRow>
          ))}
          {nextNode ? (
            <RelationRow label="Next" onActivate={go(nextNode)}>
              {shownLabel(nextNode)}
            </RelationRow>
          ) : null}
        </div>
      ) : null}
    </>
  );
}
