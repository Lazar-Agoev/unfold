/**
 * The Flow view of a card (docs/reader.md#flow-view): "Paths from written structure", a flowchart
 * diagram of the reader's Flow model. Each node shows its compiler-derived type and a plain label
 * (the board's, else a fallback name with a dotted underline), never raw code. Step groups are
 * authored: a magenta region titled with the group's title, or, collapsed, one COMBINED step. A resolved
 * call and each callback can carry a chevron that expands it in place (`expansion`): the caller draws
 * the inset in the room the layout kept under the node. A step with recorded source and a combined
 * block show `</>` (View code) on hover or when selected; Shift-click extends the selection to a run
 * of steps, and right-click (or Shift+F10 on a focused step) opens the step's menu, when the caller
 * offers them. A proposed step is drawn in the room the layout kept for it: a dashed green box with a
 * PROPOSED type row (the agent mark when an agent wrote it) and its title as plain words, joined by
 * dashed green connectors beside the solid compiled route; it is never a node, a route, or a port.
 * Geometry comes from `layoutFlow`; this component only draws it. Ports are drawn by the caller
 * (`renderPorts`).
 */

import { useId, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import type { FlowCallback, FlowModel, FlowNode } from "../ipc/generated";
import { isWithin, portStep } from "../code/outline";
import { linkNote } from "../code/text";
import { authorOf } from "../code/text";
import { Icon } from "../icons";
import { expandableCalls, type ExpandableCall } from "./insets";
import { INSIDE_ACTION, followableCall, nodeTypeName, roundedPath, type FlowLayout, type GroupStep, type PlacedInset, type PlacedNode, type PlacedProposal } from "./layout";
import { ViewCodeButton } from "./ViewCodeButton";
import "./flow.css";

/** A proposed step's author, and whether its step is missing or held for review. */
export interface ProposalMark {
  readonly author: string;
  readonly state?: "missing" | "held";
}

/** The card's proposed steps (`layout.proposals`) as drawn: their marks, the selected one, and selecting one. */
export interface FlowProposals {
  readonly marks: ReadonlyMap<string, ProposalMark>;
  readonly selected: string | null;
  readonly onSelect: (id: string) => void;
}

/** Calls that can expand in place: their chevrons, and the insets drawn under their nodes. */
export interface FlowExpansion {
  /** Whether a call shows a chevron here (null for none), and whether it is expanded. */
  readonly stateOf: (call: ExpandableCall) => "expanded" | "collapsed" | null;
  readonly onToggle: (call: ExpandableCall) => void;
  /** Draws what the layout keeps room for under a node (`layout.insets`). */
  readonly renderInset: (inset: PlacedInset) => ReactNode;
}

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
  /** The selected step group's id: its region or step shows selected. */
  readonly selectedGroup?: string | null;
  readonly onSelectGroup?: (groupId: string) => void;
  /** Collapses or expands a step group. */
  readonly onToggleGroup?: (groupId: string) => void;
  /** Opens a step group's menu (right-click on its title or step, or Shift+F10 on its toggle). */
  readonly onGroupMenu?: (groupId: string, at: MouseEvent | Element) => void;
  /** Ports by placed node id (occurrences with arrows that this node shows). */
  readonly ports?: ReadonlyMap<string, readonly string[]>;
  readonly renderPorts?: (ids: readonly string[]) => ReactNode;
  /** How far past the diagram's right edge its ports' leaders reach (an inset's, to its card's edge). */
  readonly portReach?: number;
  readonly expansion?: FlowExpansion;
  /** Drawn inside an inset: the diagram alone, without the caption and the warnings. */
  readonly inset?: boolean;
  /** Extends the selection to a run of steps ending at `key` (Shift-click); without it Shift-click selects. */
  readonly onExtend?: (key: string) => void;
  /** The steps of a Shift-selected run, by key: each shows selected. */
  readonly runKeys?: ReadonlySet<string>;
  /** Opens a step's source (its `</>`). */
  readonly onViewCode?: (key: string) => void;
  /** Opens a combined block's source (its `</>`). */
  readonly onViewGroupCode?: (groupId: string) => void;
  /**
   * Opens a step's menu (right-click on it, or Shift+F10 on it focused), once a step that wasn't
   * selected (alone or in a run) is.
   */
  readonly onStepMenu?: (key: string, at: MouseEvent | Element) => void;
  /** The card's proposed steps, when it draws them (only on its own card, never in an inset). */
  readonly proposals?: FlowProposals;
}

/** Whether a node is a step with recorded source (`</>` opens it). */
function hasSource(placed: PlacedNode): boolean {
  const { node } = placed;
  return !placed.summary && node.kind !== "start" && node.kind !== "end" && node.key !== undefined && (node.focus !== undefined || node.startLine !== undefined);
}

/** Whether a key event asks for a context menu (the menu key, or Shift+F10). */
function isMenuKey(event: KeyboardEvent): boolean {
  return event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey);
}

/**
 * The node a connection's step leaves from: the node showing its call, else the collapsed step group
 * hiding it, else the innermost node that is or holds it.
 */
function stepNode(layout: FlowLayout, step: string): PlacedNode | undefined {
  const chip = layout.nodes.find((placed) => !placed.summary && placed.node.chips?.some((item) => item.source === step));
  if (chip) {
    return chip;
  }
  const group = layout.nodes.find((placed) => placed.group?.members.some((member) => isWithin(step, member)));
  let holder: PlacedNode | undefined;
  for (const placed of layout.nodes) {
    const key = placed.node.key;
    if (!placed.summary && key !== undefined && isWithin(step, key) && (holder?.node.key === undefined || key.length > holder.node.key.length)) {
      holder = placed;
    }
  }
  return group ?? holder;
}

/**
 * Which placed node shows an occurrence (its chip's node, or the node listing the callback that owns
 * it) or a connection's step (`stepNode`).
 */
export function flowPortNodes(layout: FlowLayout, occurrences: ReadonlyMap<string, string>): { byNode: Map<string, string[]>; unplaced: string[] } {
  const byNode = new Map<string, string[]>();
  const unplaced: string[] = [];
  for (const [occurrence, owner] of occurrences) {
    const step = portStep(occurrence);
    const holder =
      step !== undefined
        ? stepNode(layout, step)
        : (layout.nodes.find((placed) => !placed.summary && placed.node.chips?.some((chip) => chip.source === occurrence)) ??
          layout.nodes.find((placed) => placed.node.callbacks?.some((callback) => callback.key === owner)));
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

/** Opens a step group's menu on right-click, when the view has one. */
function groupMenuHandler(groupId: string, props: FlowViewProps): ((event: MouseEvent) => void) | undefined {
  const { onGroupMenu } = props;
  return onGroupMenu
    ? (event) => {
        event.stopPropagation();
        onGroupMenu(groupId, event);
      }
    : undefined;
}

/** The chevron that collapses or expands a step group (the same toggle as Code's rail). */
function GroupToggle({ groupId, title, collapsed, props }: { groupId: string; title: string; collapsed: boolean; props: FlowViewProps }) {
  return (
    <button
      type="button"
      className="flow-group-toggle nodrag"
      aria-label={`${collapsed ? "Expand" : "Collapse"} step group ${title}`}
      aria-expanded={!collapsed}
      onClick={(event) => {
        event.stopPropagation();
        props.onToggleGroup?.(groupId);
      }}
      onKeyDown={(event) => {
        if (isMenuKey(event) && props.onGroupMenu) {
          event.preventDefault();
          event.stopPropagation();
          props.onGroupMenu(groupId, event.currentTarget);
        }
      }}
    >
      <Icon name={collapsed ? "chevron-right-small" : "chevron-down-small"} size={10} />
    </button>
  );
}

/** A collapsed step group: one step with the group's title and what can leave it early. */
function GroupStepView({ placed, group, selected, props }: { placed: PlacedNode; group: GroupStep; selected: boolean; props: FlowViewProps }) {
  return (
    <div
      className={`flow-node kind-group${selected ? " selected" : ""}`}
      style={{ left: placed.x, top: placed.y, width: placed.width, height: placed.height }}
      data-flow-node={placed.id}
      data-port-row="flow"
      onClick={(event) => {
        event.stopPropagation();
        props.onSelectGroup?.(group.id);
      }}
      onContextMenu={groupMenuHandler(group.id, props)}
    >
      <span className="flow-type">
        <GroupToggle groupId={group.id} title={placed.label} collapsed props={props} />
        COMBINED
        <small>
          {group.steps} {group.steps === 1 ? "step" : "steps"}
        </small>
        {props.onViewGroupCode ? (
          <ViewCodeButton
            tabbable={selected}
            onView={() => {
              props.onViewGroupCode?.(group.id);
            }}
          />
        ) : null}
      </span>
      <NodeLabel placed={placed} />
      {group.returns || group.throws ? (
        <span className="flow-group-marks">
          {group.returns ? <span className="returns">returns</span> : null}
          {group.throws ? <span className="throws">throws</span> : null}
        </span>
      ) : null}
    </div>
  );
}

/** An expanded step group's title band: the toggle, the title, and `</>`, drawn over the routes that enter it. */
function GroupHeading({ region, selected, props }: { region: FlowLayout["regions"][number]; selected: boolean; props: FlowViewProps }) {
  const groupId = region.group ?? "";
  return (
    <div
      className={`flow-group-heading${selected ? " selected" : ""}`}
      style={{ left: region.x, top: region.y, width: region.width }}
      data-group={groupId}
      onContextMenu={groupMenuHandler(groupId, props)}
    >
      <GroupToggle groupId={groupId} title={region.title} collapsed={false} props={props} />
      <span
        className="flow-group-title"
        onClick={(event) => {
          event.stopPropagation();
          props.onSelectGroup?.(groupId);
        }}
      >
        {region.title}
      </span>
      {props.onViewGroupCode ? (
        <ViewCodeButton
          tabbable={selected}
          onView={() => {
            props.onViewGroupCode?.(groupId);
          }}
        />
      ) : null}
    </div>
  );
}

/** The chevron that expands a call or a callback in place, or collapses it. */
function CallToggle({ call, state, props }: { call: ExpandableCall; state: "expanded" | "collapsed"; props: FlowViewProps }) {
  const expanded = state === "expanded";
  return (
    <button
      type="button"
      className="flow-call-toggle nodrag"
      aria-label={`${expanded ? "Collapse" : "Expand"} ${call.name} here`}
      aria-expanded={expanded}
      onClick={(event) => {
        event.stopPropagation();
        props.expansion?.onToggle(call);
      }}
    >
      <Icon name={expanded ? "chevron-down-small" : "chevron-right-small"} size={10} />
    </button>
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
    if (node.key === undefined) {
      return;
    }
    if (event.shiftKey && props.onExtend) {
      props.onExtend(node.key);
    } else {
      props.onSelect?.(node.key);
    }
  };
  // A step's own menu and keyboard focus (a click focuses it, so Shift+F10 opens its menu). The menu
  // acts on the selection: the step, unless it is already selected alone or in a run.
  const key = node.key;
  const openMenu = (step: string, at: MouseEvent | Element) => {
    if (!selected) {
      props.onSelect?.(step);
    }
    props.onStepMenu?.(step, at);
  };
  const menu =
    props.onStepMenu && key !== undefined && node.kind !== "start" && node.kind !== "end" && !placed.summary
      ? {
          tabIndex: -1,
          onMouseDown: (event: MouseEvent) => {
            // Shift-click extends the selection, never the text selection.
            if (event.shiftKey) {
              event.preventDefault();
            }
          },
          onContextMenu: (event: MouseEvent) => {
            event.stopPropagation();
            openMenu(key, event);
          },
          onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => {
            if (isMenuKey(event) && event.target === event.currentTarget) {
              event.preventDefault();
              event.stopPropagation();
              openMenu(key, event.currentTarget);
            }
          },
        }
      : {};
  const code =
    props.onViewCode && key !== undefined && hasSource(placed) ? (
      <ViewCodeButton
        tabbable={selected}
        onView={() => {
          props.onViewCode?.(key);
        }}
      />
    ) : null;
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
      <div className={classes.join(" ")} style={style} data-flow-node={placed.id} data-port-row="flow" onClick={select} {...menu}>
        {code ? <span className="flow-corner-code">{code}</span> : null}
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
  // The chevrons of the calls that can expand in place here, by the key the card saves.
  const toggles = new Map(
    (!placed.summary && props.expansion ? expandableCalls(node) : []).flatMap((call) => {
      const state = props.expansion?.stateOf(call) ?? null;
      return state ? [[call.key, { call, state }] as const] : [];
    }),
  );
  const toggle = (key: string) => {
    const found = toggles.get(key);
    return found ? <CallToggle call={found.call} state={found.state} props={props} /> : null;
  };
  return (
    <div className={classes.join(" ")} style={style} data-flow-node={placed.id} data-port-row="flow" onClick={select} {...menu}>
      {node.kind === "loop" ? (
        <svg className="flow-shape" width={placed.width} height={placed.height} viewBox={`0 0 ${placed.width} ${placed.height}`} aria-hidden="true">
          <path d={`M14 1 H${placed.width - 14} L${placed.width - 1} ${placed.height / 2} L${placed.width - 14} ${placed.height - 1} H14 L1 ${placed.height / 2} Z`} />
        </svg>
      ) : null}
      <span className="flow-type">
        <i aria-hidden="true" />
        {nodeTypeName(node)}
        {note !== null ? <small>{note.replace(/^(external|unresolved) · /, "")}</small> : null}
        {code}
      </span>
      <NodeLabel placed={placed} />
      {follow && !placed.summary ? (
        <span className="flow-call-row">
          {toggle(follow.source)}
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
        </span>
      ) : null}
      {!placed.summary
        ? node.callbacks?.map((callback) => {
            const chevron = toggle(callback.key);
            return (
              <span key={callback.key} className="flow-callback-row">
                {chevron}
                <button
                  type="button"
                  className={`flow-callback nodrag${toggles.get(callback.key)?.state === "expanded" ? " expanded" : ""}`}
                  title="Open this definition as its own card"
                  onClick={(event) => {
                    event.stopPropagation();
                    props.onOpenCallback?.(callback, event.currentTarget);
                  }}
                >
                  {chevron ? null : <span aria-hidden="true">▸</span>}
                  {callback.label}
                  <small>
                    {callback.lines} {callback.lines === 1 ? "line" : "lines"}
                    {callback.throws ? " · throws" : ""}
                  </small>
                </button>
              </span>
            );
          })
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

/**
 * A proposed step: PROPOSED (with the agent mark for an agent's), its title as plain words, and why
 * where it goes needs review; one inside a collapsed step group says so, with the action that expands it.
 */
function ProposalView({
  placed,
  mark,
  selected,
  onSelect,
  onToggleGroup,
}: {
  placed: PlacedProposal;
  mark: ProposalMark | undefined;
  selected: boolean;
  onSelect: ((id: string) => void) | undefined;
  onToggleGroup: ((groupId: string) => void) | undefined;
}) {
  const inside = placed.inside;
  const agent = mark ? authorOf(mark.author) : null;
  const classes = ["flow-proposal"];
  if (selected) classes.push("selected");
  if (mark?.state) classes.push(mark.state);
  const choose = (event: MouseEvent) => {
    event.stopPropagation();
    onSelect?.(placed.id);
  };
  return (
    <div
      className={classes.join(" ")}
      style={{ left: placed.x, top: placed.y, width: placed.width, height: placed.height }}
      data-proposal={placed.id}
      role="button"
      tabIndex={-1}
      aria-label={`Proposed step: ${placed.title}`}
      aria-pressed={selected}
      onClick={choose}
      onContextMenu={choose}
    >
      <span className="flow-type">
        <i aria-hidden="true" />
        PROPOSED
        {agent?.agent === true ? (
          <span className="flow-proposal-agent" title={`Written by ${agent.name}, an agent`}>
            <Icon name="agent" size={10} />
          </span>
        ) : null}
      </span>
      <span className="flow-proposal-title">{placed.title}</span>
      {placed.note !== undefined ? (
        <span className={`flow-proposal-note${inside !== undefined ? " inside" : ""}`}>
          {placed.note}
          {inside !== undefined && onToggleGroup ? (
            <>
              {" "}
              <button
                type="button"
                className="flow-proposal-reveal nodrag"
                onClick={(event) => {
                  event.stopPropagation();
                  onToggleGroup(inside);
                }}
              >
                {INSIDE_ACTION}
              </button>
            </>
          ) : null}
        </span>
      ) : null}
    </div>
  );
}

export function FlowView(props: FlowViewProps) {
  const { model, layout, ports, expansion } = props;
  const marker = useId().replace(/:/g, "");
  const selected = props.selectedKey ?? null;
  const selectedGroup = props.selectedGroup ?? null;
  return (
    <div className={`flow-view${props.inset === true ? " inset" : ""}`}>
      {props.inset === true ? null : <div className="flow-caption">Paths from written structure</div>}
      {props.inset !== true && model.warnings.length > 0 ? (
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
            className={`flow-region kind-${region.kind}${region.group !== undefined && region.group === selectedGroup ? " selected" : ""}`}
            style={{ left: region.x, top: region.y, width: region.width, height: region.height }}
          >
            {region.kind !== "group" ? (
              <span className="flow-region-title">
                {region.title}
                {region.subtitle !== undefined ? <small>{region.subtitle}</small> : null}
              </span>
            ) : null}
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
            <marker id={`${marker}-proposed`} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
              <path d="M0 0 L10 5 L0 10 Z" className="flow-arrowhead proposed" />
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
          {props.proposals
            ? layout.proposalEdges.map((edge) => (
                <path key={edge.id} className="flow-proposal-edge" d={roundedPath(edge.points)} markerEnd={`url(#${marker}-proposed)`} data-proposal-edge={edge.proposal} />
              ))
            : null}
        </svg>
        {layout.regions.map((region) =>
          region.kind === "group" ? (
            <GroupHeading key={`heading-${region.group ?? ""}`} region={region} selected={region.group !== undefined && region.group === selectedGroup} props={props} />
          ) : null,
        )}
        {expansion
          ? layout.insets.map((inset) => (
              <div
                key={`inset-${inset.node}`}
                className="flow-inset-slot"
                data-inset-node={inset.key}
                style={{ left: inset.x, top: inset.y, width: inset.width, height: inset.height }}
              >
                {expansion.renderInset(inset)}
              </div>
            ))
          : null}
        {layout.nodes.map((placed) => {
          const nodePorts = ports?.get(placed.id);
          return (
            <FlowNodeWithPorts
              key={placed.id}
              placed={placed}
              selected={
                placed.group
                  ? placed.group.id === selectedGroup
                  : !placed.summary && placed.node.key !== undefined && (placed.node.key === selected || props.runKeys?.has(placed.node.key) === true)
              }
              props={props}
              ports={nodePorts}
              diagramWidth={Math.max(layout.width, props.width)}
            />
          );
        })}
        {props.proposals
          ? layout.proposals.map((placed) => (
              <ProposalView
                key={placed.id}
                placed={placed}
                mark={props.proposals?.marks.get(placed.id)}
                selected={props.proposals?.selected === placed.id}
                onSelect={props.proposals?.onSelect}
                onToggleGroup={props.onToggleGroup}
              />
            ))
          : null}
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
          className={`flow-leader${ports.every((port) => portStep(port) !== undefined) ? " authored" : ""}`}
          style={{ left: placed.x + placed.width, top: placed.y + Math.min(placed.height / 2, 18), width: Math.max(0, diagramWidth - placed.x - placed.width) + (props.portReach ?? 0) }}
        >
          {props.renderPorts(ports)}
        </div>
      ) : null}
      {placed.group ? (
        <GroupStepView placed={placed} group={placed.group} selected={selected} props={props} />
      ) : (
        <FlowNodeView placed={placed} selected={selected} props={props} />
      )}
    </>
  );
}
