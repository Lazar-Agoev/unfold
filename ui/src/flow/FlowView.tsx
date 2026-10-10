/**
 * The Flow view of a card (architecture §10.4): "Paths from written structure", a Mermaid-style
 * diagram of the reader's Flow model. Each node shows its compiler-derived type and a plain label
 * (the board's, else a fallback name with a dotted underline), never raw code. Geometry comes from
 * `layoutFlow`; this component only draws it. Ports are drawn by the caller (`renderPorts`).
 */

import { useId, type MouseEvent, type ReactNode } from "react";
import type { FlowCallback, FlowModel, FlowNode } from "../ipc/generated";
import { linkNote } from "../code/text";
import { Icon } from "../icons";
import { followableCall, nodeTypeName, roundedPath, type FlowLayout, type PlacedNode } from "./layout";
import "./flow.css";

export interface FlowViewProps {
  readonly model: FlowModel;
  readonly layout: FlowLayout;
  /** The card's width, so ports can sit at its right edge. */
  readonly width: number;
  readonly selectedKey?: string | null;
  readonly onSelect?: (key: string) => void;
  /** Follows a node's main call. */
  readonly onFollow?: (node: FlowNode, target: string, anchor: HTMLElement) => void;
  readonly onOpenCallback?: (callback: FlowCallback, anchor: HTMLElement) => void;
  readonly onToggleChain?: (key: string) => void;
  /** Ports by placed node id (occurrences with arrows that this node shows). */
  readonly ports?: ReadonlyMap<string, readonly string[]>;
  readonly renderPorts?: (ids: readonly string[]) => ReactNode;
}

/** Which placed node shows an occurrence: its chip's node, or the node listing the callback that owns it. */
export function flowPortNodes(layout: FlowLayout, occurrences: ReadonlyMap<string, string>): { byNode: Map<string, string[]>; unplaced: string[] } {
  const byNode = new Map<string, string[]>();
  const unplaced: string[] = [];
  for (const [occurrence, owner] of occurrences) {
    const holder =
      layout.nodes.find((placed) => !placed.summary && placed.node.chips?.some((chip) => chip.source === occurrence)) ??
      layout.nodes.find((placed) => placed.node.callbacks?.some((callback) => callback.key === owner));
    if (holder) {
      const list = byNode.get(holder.id) ?? [];
      list.push(occurrence);
      byNode.set(holder.id, list);
    } else {
      unplaced.push(occurrence);
    }
  }
  return { byNode, unplaced };
}

function NodeLabel({ placed }: { placed: PlacedNode }) {
  return (
    <span className={`flow-label${placed.fallback ? " fallback" : ""}`} title={placed.fallback ? "A name from the code; no label yet" : undefined}>
      {placed.label}
    </span>
  );
}

function FlowNodeView({
  placed,
  selected,
  props,
}: {
  placed: PlacedNode;
  selected: boolean;
  props: FlowViewProps;
}) {
  const { node } = placed;
  const style = { left: placed.x, top: placed.y, width: placed.width, height: placed.height };
  const select = (event: MouseEvent) => {
    event.stopPropagation();
    if (node.key !== undefined) {
      props.onSelect?.(node.key);
    }
  };
  const classes = ["flow-node", `kind-${node.kind}`];
  if (selected) classes.push("selected");
  if (placed.summary) classes.push("summary");
  const main = node.call !== undefined ? node.chips?.[node.call] : undefined;
  if (main) classes.push(main.resolution);

  if (node.kind === "start" || node.kind === "end") {
    return (
      <div className={`flow-pill kind-${node.kind}${selected ? " selected" : ""}`} style={style} data-flow-node={placed.id} data-port-row="flow" onClick={select}>
        <span className={node.kind === "start" ? "flow-pill-name" : undefined}>{placed.label}</span>
      </div>
    );
  }
  if (node.kind === "condition") {
    return (
      <div className={classes.join(" ")} style={style} data-flow-node={placed.id} data-port-row="flow" onClick={select}>
        <svg className="flow-shape" width={placed.width} height={placed.height} viewBox={`0 0 ${placed.width} ${placed.height}`} aria-hidden="true">
          <path d={`M${placed.width / 2} 1 L${placed.width - 1} ${placed.height / 2} L${placed.width / 2} ${placed.height - 1} L1 ${placed.height / 2} Z`} />
        </svg>
        <div className="flow-diamond-text">
          <span className="flow-type">{nodeTypeName(node)}</span>
          <NodeLabel placed={placed} />
          {node.checks !== undefined && node.key !== undefined ? (
            <button
              type="button"
              className="flow-checks nodrag"
              aria-expanded={placed.chainFolded === false}
              onClick={(event) => {
                event.stopPropagation();
                if (node.key !== undefined) {
                  props.onToggleChain?.(node.key);
                }
              }}
            >
              {placed.chainFolded === false ? "▾" : "▸"} {node.checks} checks
            </button>
          ) : null}
        </div>
      </div>
    );
  }
  const follow = followableCall(node);
  const note = main && main.resolution !== "resolved" ? linkNote({ resolution: main.resolution, ...(main.note !== undefined ? { note: main.note } : {}) }) : null;
  return (
    <div className={classes.join(" ")} style={style} data-flow-node={placed.id} data-port-row="flow" onClick={select}>
      {node.kind === "loop" ? (
        <svg className="flow-shape" width={placed.width} height={placed.height} viewBox={`0 0 ${placed.width} ${placed.height}`} aria-hidden="true">
          <path d={`M14 1 H${placed.width - 14} L${placed.width - 1} ${placed.height / 2} L${placed.width - 14} ${placed.height - 1} H14 L1 ${placed.height / 2} Z`} />
        </svg>
      ) : null}
      <span className="flow-type">
        <i aria-hidden="true" />
        {nodeTypeName(node)}
        {note !== null ? <small>{note.replace(/^(external|unresolved) · /, "")}</small> : null}
      </span>
      <NodeLabel placed={placed} />
      {follow && !placed.summary ? (
        <button
          type="button"
          className="flow-callee code-link resolved nodrag"
          aria-label={`Follow ${follow.name}`}
          onClick={(event) => {
            event.stopPropagation();
            if (follow.target !== undefined) {
              props.onFollow?.(node, follow.target, event.currentTarget);
            }
          }}
        >
          → {follow.name}
        </button>
      ) : null}
      {!placed.summary
        ? node.callbacks?.map((callback) => (
            <button
              key={callback.key}
              type="button"
              className="flow-callback nodrag"
              title="Open this definition as its own card"
              onClick={(event) => {
                event.stopPropagation();
                props.onOpenCallback?.(callback, event.currentTarget);
              }}
            >
              <span aria-hidden="true">▸</span>
              {callback.label}
              <small>
                {callback.lines} {callback.lines === 1 ? "line" : "lines"}
                {callback.throws ? " · throws" : ""}
              </small>
            </button>
          ))
        : null}
      {node.kind === "opaque" ? (
        <span className="flow-opaque-note">
          <Icon name="warning-triangle" size={12} />
          Routing not drawn · see Code
        </span>
      ) : null}
    </div>
  );
}

export function FlowView(props: FlowViewProps) {
  const { model, layout, ports } = props;
  const marker = useId().replace(/:/g, "");
  const selected = props.selectedKey ?? null;
  return (
    <div className="flow-view">
      <div className="flow-caption">Paths from written structure</div>
      {model.warnings.length > 0 ? (
        <div className="flow-warnings" role="note">
          <Icon name="warning-triangle" size={14} />
          <div>
            {model.warnings.map((warning) => (
              <p key={warning.code}>{warning.message}</p>
            ))}
          </div>
        </div>
      ) : null}
      <div className="flow-diagram" role="group" aria-label="Flow diagram" style={{ width: Math.max(layout.width, props.width), height: layout.height }}>
        {layout.regions.map((region, index) => (
          <div
            key={`${region.kind}-${index}`}
            className={`flow-region kind-${region.kind}`}
            style={{ left: region.x, top: region.y, width: region.width, height: region.height }}
          >
            <span className="flow-region-title">
              {region.title}
              {region.subtitle !== undefined ? <small>{region.subtitle}</small> : null}
            </span>
          </div>
        ))}
        <svg className="flow-edges" width={Math.max(layout.width, props.width)} height={layout.height} aria-hidden="true">
          <defs>
            <marker id={`${marker}-arrow`} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
              <path d="M0 0 L10 5 L0 10 Z" className="flow-arrowhead" />
            </marker>
            <marker id={`${marker}-error`} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
              <path d="M0 0 L10 5 L0 10 Z" className="flow-arrowhead error" />
            </marker>
          </defs>
          {layout.edges.map((edge) => (
            <g key={edge.id} className={`flow-edge kind-${edge.kind}${edge.dashed ? " dashed" : ""}`}>
              <path d={roundedPath(edge.points)} markerEnd={`url(#${marker}-${edge.dashed ? "error" : "arrow"})`} />
              {edge.label !== null && edge.labelAt ? (
                <text x={edge.labelAt.x} y={edge.labelAt.y} className="flow-edge-label">
                  {edge.label}
                </text>
              ) : null}
            </g>
          ))}
        </svg>
        {layout.nodes.map((placed) => {
          const nodePorts = ports?.get(placed.id);
          return (
            <FlowNodeWithPorts
              key={placed.id}
              placed={placed}
              selected={selected !== null && placed.node.key === selected && !placed.summary}
              props={props}
              ports={nodePorts}
              diagramWidth={Math.max(layout.width, props.width)}
            />
          );
        })}
      </div>
    </div>
  );
}

function FlowNodeWithPorts({
  placed,
  selected,
  props,
  ports,
  diagramWidth,
}: {
  placed: PlacedNode;
  selected: boolean;
  props: FlowViewProps;
  ports: readonly string[] | undefined;
  diagramWidth: number;
}) {
  return (
    <>
      {ports && ports.length > 0 && props.renderPorts ? (
        <div
          className="flow-leader"
          style={{ left: placed.x + placed.width, top: placed.y + Math.min(placed.height / 2, 18), width: Math.max(0, diagramWidth - placed.x - placed.width) }}
        >
          {props.renderPorts(ports)}
        </div>
      ) : null}
      <FlowNodeView placed={placed} selected={selected} props={props} />
    </>
  );
}
