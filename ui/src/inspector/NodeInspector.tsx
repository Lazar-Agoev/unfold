/**
 * The inspector for a selected Flow node (architecture §11.5): its type and lines, its Flow label
 * (showing the name from the code it replaces), and where its yes and no paths go. Never code.
 */

import type { Card, FlowModel, FlowNode, RouteKind } from "../ipc/generated";
import { nodeTypeName } from "../flow/layout";
import { labelBlock } from "../state/boards/api";
import { cardKey, keyDisplayName } from "../state/boards/document";
import { itemsFor } from "../state/boards/health";
import { selectBlock, useBoards } from "../state/boards/store";
import { AuthoredField } from "./AuthoredField";
import { Section } from "./CardInspector";
import { IssueLine } from "./IssueLine";
import { useAuthor } from "./useAuthor";

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

export interface NodeInspectorProps {
  readonly boardId: string;
  readonly card: Card;
  readonly node: FlowNode;
  /** The card's Flow model without the board's labels: its fallback names. */
  readonly model: FlowModel;
}

export function NodeInspector({ boardId, card, node, model }: NodeInspectorProps) {
  const key = node.key ?? "";
  const author = useAuthor();
  const labels = useBoards((state) => state.boards[boardId]?.document.labels);
  const health = useBoards((state) => state.boards[boardId]?.health);
  const issues = itemsFor(health, { label: key });
  const held = issues.some((item) => item.kind === "review");
  const shownLabel = (target: FlowNode) => (target.key !== undefined ? (labels?.[target.key]?.text ?? target.label) : target.label);
  const lines =
    node.startLine !== undefined ? (node.endLine !== undefined && node.endLine !== node.startLine ? `lines ${node.startLine}–${node.endLine}` : `line ${node.startLine}`) : "";
  const paths = model.routes
    .filter((route) => route.from === node.id && route.kind !== "sequence")
    .map((route) => ({ route, target: model.nodes[route.to] }))
    .filter((path): path is { route: typeof path.route; target: FlowNode } => path.target !== undefined);
  const next = model.routes.find((route) => route.from === node.id && route.kind === "sequence");
  const nextNode = next ? model.nodes[next.to] : undefined;
  const fallback = node.fallback === true ? node.label : null;

  return (
    <div className="inspector-body">
      <header className="inspector-title">
        <h3>{shownLabel(node)}</h3>
        <p>
          <span className="inspector-kind">{nodeTypeName(node)}</span>
          {lines}
          {" · in "}
          <button
            type="button"
            className="inspector-link"
            onClick={() => {
              selectBlock(boardId, card.id, cardKey(card));
            }}
          >
            {keyDisplayName(cardKey(card))}
          </button>
        </p>
      </header>
      <AuthoredField
        label="Flow label"
        value={labels?.[key]}
        placeholder={fallback ?? "A plain-language label…"}
        focusKey="inspector.label"
        disabledReason={held ? "Held for review: accept or reconnect it before labeling it." : author === null ? "Loading who you are…" : null}
        status={
          <>
            {fallback !== null ? (
              <span className="inspector-fallback">
                Replaces the name from the code: <span className="code-name fallback">{fallback}</span>
              </span>
            ) : null}
            {issues.map((item) => (
              <IssueLine key={item.message} boardId={boardId} item={item} />
            ))}
          </>
        }
        onChange={(text) => {
          if (author !== null && key.length > 0) {
            labelBlock(boardId, key, text, author);
          }
        }}
      />
      {paths.length > 0 || nextNode ? (
        <Section title="Paths">
          <ul className="inspector-list">
            {paths.map(({ route, target }) => (
              <li key={`${route.kind}:${route.to}`}>
                <button
                  type="button"
                  className="inspector-row"
                  disabled={target.key === undefined}
                  onClick={() => {
                    if (target.key !== undefined) {
                      selectBlock(boardId, card.id, target.key);
                    }
                  }}
                >
                  <span className={`inspector-path kind-${route.kind}`}>{PATH_WORDS[route.kind] ?? route.kind}</span>
                  <span className="inspector-name">{shownLabel(target)}</span>
                </button>
              </li>
            ))}
            {nextNode && paths.length === 0 ? (
              <li>
                <button
                  type="button"
                  className="inspector-row"
                  disabled={nextNode.key === undefined}
                  onClick={() => {
                    if (nextNode.key !== undefined) {
                      selectBlock(boardId, card.id, nextNode.key);
                    }
                  }}
                >
                  <span className="inspector-path">Next</span>
                  <span className="inspector-name">{shownLabel(nextNode)}</span>
                </button>
              </li>
            ) : null}
          </ul>
        </Section>
      ) : null}
    </div>
  );
}
