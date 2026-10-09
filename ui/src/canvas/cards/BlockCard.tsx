/**
 * A block card: one definition (or another indexed block) of a project in Code or Flow. The header
 * shows the kind, name, and location, the card's own Code/Flow switch (saved with the board), and the
 * fold toggle; a folded card shows only its title and its arrows attach to the header. The body never
 * scrolls: the card sizes itself to its content. Flow shows the page's labels for the card's project.
 */

import { useQuery } from "@tanstack/react-query";
import { useUpdateNodeInternals } from "@xyflow/react";
import { AlertTriangle } from "lucide-react";
import { useLayoutEffect, useMemo, type ReactNode } from "react";
import { SegmentedControl, showToast } from "../../components";
import { CodeView } from "../../code/CodeView";
import { defaultFolds, groupConflict, groupSelection } from "../../code/outline";
import { shortPath } from "../../code/text";
import { FlowView, flowPortNodes } from "../../flow/FlowView";
import { layoutFlow } from "../../flow/layout";
import type { CallLink, Card, CodeText, CodeView as CodeViewModel, ProjectId, View } from "../../ipc/generated";
import {
  addGroup,
  editPage,
  findPage,
  followCall,
  keyDisplayName,
  labelsFor,
  lineOffsetIn,
  select,
  setCardView,
  setCardsFolded,
  setPendingGroup,
  updateGroup,
  updatePageView,
  useOpenBoard,
  usePageView,
  type PageTarget,
} from "../../state/boards";
import { cardQuery, isMissing, lastVerifiedCode } from "../cardData";
import { ProjectScope } from "../ProjectScope";
import { CARD_MAX_WIDTH, CARD_MIN_WIDTH } from "../layout";
import { useMeasure } from "../measure";
import { blockCardWidth, codeContentWidth } from "../sizing";
import { useCardMenu } from "./cardMenu";
import { InlineCallee } from "./InlineCallee";
import { DescriptionLine, FoldToggle, HeaderPorts, KindIcon, TargetHandle, renderPorts } from "./parts";

const VIEW_OPTIONS = [
  { value: "code", label: "Code" },
  { value: "flow", label: "Flow" },
] as const;

const NO_FOLDS: readonly string[] = [];
const NO_KEYS: readonly string[] = [];

export interface BlockCardProps {
  readonly target: PageTarget;
  /** The project the card resolves to. */
  readonly project: ProjectId;
  readonly card: Card & { readonly ref: { readonly block: string } };
  readonly view: View;
  readonly ports: readonly string[];
  readonly selected: boolean;
}

function toggled(list: readonly string[], key: string): string[] {
  return list.includes(key) ? list.filter((item) => item !== key) : [...list, key];
}

export function BlockCard({ target, project, card, view, ports, selected }: BlockCardProps) {
  const key = card.ref.block;
  const query = useQuery(cardQuery(project, key));
  const model = query.data?.type === "block" ? query.data : undefined;
  const measure = useMeasure();
  const updateNodeInternals = useUpdateNodeInternals();
  const folded = card.folded === true;
  const header = model?.header;
  const definition = useMemo(
    () =>
      header ? { path: header.path, kind: header.kind, name: header.name, qualifiedName: header.qualifiedName, line: header.startLine } : null,
    [header],
  );
  const menu = useCardMenu(target, project, card.id, folded, definition);

  const codeFolds = usePageView(target, (pageView) => pageView.codeFolds[card.id]);
  const expandedKeys = usePageView(target, (pageView) => pageView.expanded[card.id] ?? NO_KEYS);
  const openChains = usePageView(target, (pageView) => pageView.flowChains[card.id] ?? NO_KEYS);
  const selectedKey = usePageView(target, ({ selection }) => (selection.kind === "card" && selection.cardId === card.id ? selection.key : null));
  const pending = usePageView(target, ({ pendingGroup }) => (pendingGroup?.cardId === card.id ? pendingGroup : null));
  const allGroups = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.groups);
  const labels = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.labels);
  const health = useOpenBoard(target.board, (board) => board.health);
  const groups = useMemo(() => {
    const broken = new Set(
      (health?.items ?? []).flatMap((item) => (item.page === target.page && item.kind === "brokenGroup" && "group" in item.target ? [item.target.group] : [])),
    );
    return (allGroups ?? []).filter((group) => group.card === card.id && !broken.has(group.id));
  }, [allGroups, health, card.id, target.page]);

  // Code: the verified source, or the last verified render kept in memory while the file changed.
  const codeModel: CodeViewModel | null = model?.code ?? null;
  const stale = codeModel?.source.state === "changed" ? lastVerifiedCode(project, key) : undefined;
  const shown: CodeText | null =
    codeModel?.source.state === "current" ? codeModel.source : stale?.source.state === "current" ? stale.source : null;
  const folds = useMemo(() => new Set(codeFolds ?? (shown ? defaultFolds(shown.outline) : NO_FOLDS)), [codeFolds, shown]);
  const portSet = useMemo(() => new Set(ports), [ports]);
  // The page's labels for this card's project (`ref.project` names it the same way labels do).
  const labelText = useMemo(() => labelsFor(labels ?? [], card.ref.project), [labels, card.ref.project]);
  const chainSet = useMemo(() => new Set(openChains), [openChains]);

  // Laid out only while the card shows Flow: measuring and placing nodes is the costly part of a card.
  const flowModel = view === "flow" ? (model?.flow ?? null) : null;
  const flowLayout = useMemo(
    () => (flowModel ? layoutFlow(flowModel, { labels: labelText, openChains: chainSet, measure, maxWidth: CARD_MAX_WIDTH - 2 }) : null),
    [flowModel, labelText, chainSet, measure],
  );

  const name = model?.header.qualifiedName ?? keyDisplayName(key);
  const location = model ? `${shortPath(model.header.path)}:${model.header.startLine}` : "";
  const portLines = useMemo(() => {
    const lines = new Set<number>();
    for (const link of shown?.links ?? []) {
      if (portSet.has(link.source)) {
        lines.add(link.line);
      }
    }
    return lines;
  }, [shown, portSet]);
  const bodyWidth = folded
    ? null
    : view === "flow" && flowLayout
      ? flowLayout.width + 2
      : shown
        ? codeContentWidth(shown, portLines, measure)
        : null;
  // `measure` changes when the bundled fonts load, so widths come from the real fonts.
  const width = useMemo(() => blockCardWidth({ name, location, body: bodyWidth }, measure), [name, location, bodyWidth, measure]);

  // Which ports the body places; the rest (and all of a folded card's) sit on the header.
  const flowPorts = useMemo(() => {
    if (view !== "flow" || !flowLayout) {
      return null;
    }
    const owners = new Map<string, string>();
    for (const id of ports) {
      owners.set(id, shown?.links.find((link) => link.source === id)?.owner ?? "");
    }
    return flowPortNodes(flowLayout, owners);
  }, [view, flowLayout, ports, shown]);
  const headerPorts = folded
    ? ports
    : view === "flow" && flowPorts
      ? flowPorts.unplaced
      : shown
        ? ports.filter((id) => !shown.links.some((link) => link.source === id))
        : ports;

  const portsKey = ports.join("\u0000");
  const foldsKey = [...folds].join("\u0000");
  useLayoutEffect(() => {
    updateNodeInternals(card.id);
  }, [updateNodeInternals, card.id, portsKey, foldsKey, folded, view, width, model, expandedKeys, openChains, groups, pending]);

  const selectKey = (blockKey: string) => {
    select(target, { kind: "card", cardId: card.id, key: blockKey });
  };
  const follow = (targetKey: string, anchor: HTMLElement, line?: number) => {
    const offset = lineOffsetIn(anchor);
    followCall({
      target,
      fromCardId: card.id,
      fromLabel: name,
      targetKey,
      targetLabel: keyDisplayName(targetKey),
      ...(offset !== undefined ? { lineOffset: offset } : {}),
      ...(line !== undefined ? { line } : {}),
    });
  };
  const onFollowLink = (link: CallLink, anchor: HTMLElement) => {
    if (link.target !== undefined) {
      follow(link.target, anchor, link.line);
    }
  };
  const onExpand = (link: CallLink) => {
    updatePageView(target, (draft) => {
      draft.expanded[card.id] = toggled(draft.expanded[card.id] ?? [], link.source);
    });
  };
  const expanded = useMemo(() => {
    const map = new Map<string, ReactNode>();
    for (const occurrence of expandedKeys) {
      const callee = shown?.links.find((link) => link.source === occurrence)?.target;
      if (callee !== undefined) {
        map.set(
          occurrence,
          <InlineCallee
            project={project}
            targetKey={callee}
            onClose={() => {
              updatePageView(target, (draft) => {
                draft.expanded[card.id] = (draft.expanded[card.id] ?? []).filter((item) => item !== occurrence);
              });
            }}
          />,
        );
      }
    }
    return map;
  }, [expandedKeys, shown, target, project, card.id]);

  const onGutterSelect = (from: number, to: number) => {
    if (!shown) {
      return;
    }
    const selection = groupSelection(shown.outline, from, to);
    if ("error" in selection) {
      setPendingGroup(target, null);
      showToast({ title: selection.error });
      return;
    }
    const conflict = groupConflict(shown.outline, selection.members, groups);
    setPendingGroup(target, { cardId: card.id, members: selection.members, ...(conflict !== null ? { error: conflict } : {}) });
  };
  const onNamePendingGroup = (title: string | null) => {
    if (title === null || title.trim().length === 0 || !pending || pending.error !== undefined) {
      setPendingGroup(target, null);
      return;
    }
    editPage(target, (page) => addGroup(page, card.id, pending.members, title.trim()));
    setPendingGroup(target, null);
  };

  const setView = (next: View) => {
    editPage(target, (page) => {
      setCardView(page, card.id, next);
    });
    updatePageView(target, (draft) => {
      if (draft.lens !== null && !draft.lensExceptions.includes(card.id)) {
        draft.lensExceptions.push(card.id);
      }
    });
  };
  const toggleFolded = () => {
    editPage(target, (page) => {
      setCardsFolded(page, [card.id], !folded);
    });
  };

  let body: ReactNode;
  if (query.isPending) {
    body = <p className="card-note">Loading…</p>;
  } else if (query.isError) {
    body = (
      <p className="card-note warn">
        <AlertTriangle size={13} aria-hidden="true" />
        {isMissing(query.error) ? "This reference is missing from the graph." : query.error.message}
      </p>
    );
  } else if (view === "flow" && model?.flow && flowLayout && flowPorts) {
    body = (
      <FlowView
        model={model.flow}
        layout={flowLayout}
        width={width - 2}
        selectedKey={selectedKey ?? null}
        onSelect={selectKey}
        onFollow={(_node, target, anchor) => {
          follow(target, anchor);
        }}
        onOpenCallback={(callback, anchor) => {
          follow(callback.key, anchor);
        }}
        onToggleChain={(chainKey) => {
          updatePageView(target, (draft) => {
            draft.flowChains[card.id] = toggled(draft.flowChains[card.id] ?? [], chainKey);
          });
        }}
        ports={flowPorts.byNode}
        renderPorts={renderPorts}
      />
    );
  } else if (shown && model) {
    body = (
      <>
        {stale ? (
          <p className="card-note warn">
            <AlertTriangle size={13} aria-hidden="true" />
            {model.header.fileState === "last_good" ? "Doesn't parse right now · showing the last verified code" : "Updating · showing the last verified code"}
          </p>
        ) : null}
        <CodeView
          code={shown}
          language={model.header.language}
          label={name}
          folds={folds}
          onToggleFold={(foldKey) => {
            updatePageView(target, (draft) => {
              draft.codeFolds[card.id] = toggled(draft.codeFolds[card.id] ?? [...folds], foldKey);
            });
          }}
          groups={groups}
          onToggleGroup={(groupId) => {
            const group = groups.find((item) => item.id === groupId);
            editPage(target, (page) => {
              updateGroup(page, groupId, { collapsed: !(group?.collapsed ?? false) });
            });
          }}
          onRenameGroup={(groupId, title) => {
            editPage(target, (page) => {
              updateGroup(page, groupId, { title });
            });
          }}
          onGroupMenu={menu.openGroupMenu}
          ports={portSet}
          renderPorts={renderPorts}
          selectedKey={selectedKey ?? null}
          onSelect={selectKey}
          onFollow={onFollowLink}
          onExpand={onExpand}
          expanded={expanded}
          onGutterSelect={onGutterSelect}
          pendingGroup={pending}
          onNamePendingGroup={onNamePendingGroup}
          dimmed={stale !== undefined}
        />
      </>
    );
  } else if (codeModel?.source.state === "changed") {
    body = (
      <p className="card-note warn">
        <AlertTriangle size={13} aria-hidden="true" />
        {model?.header.fileState === "last_good" ? "This file doesn't parse right now." : "The source changed · updating."}
      </p>
    );
  } else {
    body = (
      <p className="card-note warn">
        <AlertTriangle size={13} aria-hidden="true" />
        Source unavailable.
      </p>
    );
  }

  const missing = query.isError && isMissing(query.error);
  const freshness = model && model.header.source !== "current" ? (model.header.source === "changed" ? "changed" : "missing") : null;
  return (
    <article
      className={`board-card block-card${selected ? " selected" : ""}${folded ? " folded" : ""}${missing ? " placeholder" : ""}`}
      data-card-id={card.id}
      data-key={key}
      data-loading={query.isPending ? "true" : undefined}
      style={{ width: Math.max(CARD_MIN_WIDTH, width) }}
      aria-label={name}
      onClick={() => {
        selectKey(key);
      }}
      onContextMenu={menu.onContextMenu}
    >
      <TargetHandle />
      <header className="card-header card-drag" onDoubleClick={toggleFolded}>
        <FoldToggle folded={folded} label={name} onToggle={toggleFolded} />
        <KindIcon kind={model?.header.kind ?? null} />
        <div className="card-title">
          <h3 title={model?.header.qualifiedName ?? key}>{name}</h3>
          {location ? <span className="card-location">{location}</span> : null}
        </div>
        {freshness ? (
          <span className={`card-freshness ${freshness}`}>
            <AlertTriangle size={11} aria-hidden="true" />
            {freshness === "changed" ? "Source changed" : "Source missing"}
          </span>
        ) : null}
        <span className="card-header-spacer" />
        {!folded && model ? (
          <div className="nodrag" onClick={(event) => { event.stopPropagation(); }}>
            <SegmentedControl options={VIEW_OPTIONS} value={view} onChange={setView} label={`View of ${name}`} variant="mini" />
          </div>
        ) : null}
        <HeaderPorts ids={headerPorts} />
      </header>
      {folded ? null : (
        <>
          <DescriptionLine description={card.description} />
          <div className="card-body nodrag nopan">
            <ProjectScope.Provider value={project}>{body}</ProjectScope.Provider>
          </div>
        </>
      )}
      {menu.element}
    </article>
  );
}
