/**
 * An entrance card (architecture §10.2): the declared entrance, its evidence, and its targets in
 * registration order, without numbers. A resolved target follows like a call; its arrow leaves from
 * its row.
 */

import { useQuery } from "@tanstack/react-query";
import { Handle, Position, useUpdateNodeInternals } from "@xyflow/react";
import { AlertTriangle } from "lucide-react";
import { useLayoutEffect, useMemo, useState } from "react";
import { CodeView } from "../../code/CodeView";
import { defaultFolds } from "../../code/outline";
import { reasonText, shortPath } from "../../code/text";
import type { Card, CardView, TargetView } from "../../ipc/generated";
import { entranceTitle } from "../../state/boards/api";
import { keyDisplayName, setCardsFolded } from "../../state/boards/document";
import { followCall, lineOffsetIn } from "../../state/boards/navigation";
import { editBoard, selectCard } from "../../state/boards/store";
import { select } from "../../state/selection";
import { targetHandle } from "../arrows";
import { cardQuery, isMissing } from "../cardData";
import { useMeasure } from "../measure";
import { entranceCardWidth } from "../sizing";
import { useCardMenu } from "./cardMenu";
import { DescriptionLine, FoldToggle, HeaderPorts, TargetHandle } from "./parts";

type EntranceModel = Extract<CardView, { type: "entrance" }>;

export interface EntranceCardProps {
  readonly boardId: string;
  readonly card: Card & { readonly ref: { readonly entrance: string } };
  readonly ports: readonly string[];
  readonly selected: boolean;
}

function targetName(target: TargetView): string {
  return target.name ?? target.text;
}

function TargetRow({
  target,
  port,
  onFollow,
}: {
  readonly target: TargetView;
  readonly port: string | null;
  readonly onFollow: (target: string, anchor: HTMLElement) => void;
}) {
  const followable = target.resolution === "resolved" && target.target !== null;
  const note =
    target.resolution === "external"
      ? (target.module ?? target.note ?? "external")
      : target.resolution === "unresolved"
        ? (target.note ?? reasonText(target.reason))
        : target.path !== null
          ? `${shortPath(target.path)}${target.line !== null ? `:${target.line}` : ""}`
          : null;
  return (
    <li className={`ecard-target ${target.resolution}`} data-port-row="target" data-target-key={target.target ?? undefined}>
      <span className="ecard-role">{target.role}</span>
      {followable ? (
        <button
          type="button"
          className="code-link resolved nodrag"
          onClick={(event) => {
            event.stopPropagation();
            if (target.target !== null) {
              onFollow(target.target, event.currentTarget);
            }
          }}
        >
          {targetName(target)}
        </button>
      ) : (
        <span className={`code-link ${target.resolution}`} title={note ?? undefined}>
          {targetName(target)}
        </span>
      )}
      {note !== null ? <small>{note}</small> : null}
      {port !== null ? (
        <>
          <span className="code-leader" aria-hidden="true" />
          <Handle id={port} type="source" position={Position.Right} isConnectable={false} className="card-port" />
        </>
      ) : null}
    </li>
  );
}

function Evidence({ model }: { readonly model: EntranceModel }) {
  const { evidence } = model;
  const code = evidence.code?.source.state === "current" ? evidence.code : null;
  const initial = useMemo(() => (code?.source.state === "current" ? defaultFolds(code.source.outline) : []), [code]);
  const [folds, setFolds] = useState<ReadonlySet<string> | null>(null);
  const location = `${evidence.path}${evidence.startLine !== null ? `:${evidence.startLine}` : ""}`;
  if (evidence.source !== "current") {
    return (
      <div className="ecard-evidence">
        <span className="ecard-location">{location}</span>
        <p className="card-note warn">
          <AlertTriangle size={13} aria-hidden="true" />
          {evidence.source === "changed" ? "The declaration changed · updating." : "Source unavailable."}
        </p>
      </div>
    );
  }
  return (
    <div className="ecard-evidence">
      <span className="ecard-location">{location}</span>
      {code?.source.state === "current" ? (
        <CodeView
          code={code.source}
          language={code.language}
          label={location}
          folds={folds ?? new Set(initial)}
          onToggleFold={(key) => {
            const next = new Set(folds ?? initial);
            if (next.has(key)) {
              next.delete(key);
            } else {
              next.add(key);
            }
            setFolds(next);
          }}
        />
      ) : evidence.text !== null ? (
        <pre className="ecard-text">{evidence.text}</pre>
      ) : null}
    </div>
  );
}

export function EntranceCard({ boardId, card, ports, selected }: EntranceCardProps) {
  const key = card.ref.entrance;
  const query = useQuery(cardQuery(key));
  const model = query.data?.type === "entrance" ? query.data : undefined;
  const measure = useMeasure();
  const updateNodeInternals = useUpdateNodeInternals();
  const folded = card.folded === true;
  const menu = useCardMenu(boardId, card.id, key, folded);
  const title = model ? entranceTitle(model) : keyDisplayName(key);
  const portSet = useMemo(() => new Set(ports), [ports]);

  const width = useMemo(() => {
    const lines =
      model?.evidence.code?.source.state === "current"
        ? model.evidence.code.source.text.split("\n")
        : (model?.evidence.text?.split("\n") ?? []);
    return entranceCardWidth(title, lines, (model?.targets ?? []).map(targetName), measure);
  }, [model, title, measure]);

  const portsKey = ports.join("\u0000");
  useLayoutEffect(() => {
    updateNodeInternals(card.id);
  }, [updateNodeInternals, card.id, portsKey, folded, width, model]);

  const toggleFolded = () => {
    editBoard(boardId, (document) => {
      setCardsFolded(document, [card.id], !folded);
    });
  };
  const follow = (target: string, anchor: HTMLElement) => {
    const offset = lineOffsetIn(anchor);
    followCall({
      boardId,
      fromCardId: card.id,
      fromLabel: title,
      targetKey: target,
      targetLabel: keyDisplayName(target),
      ...(offset !== undefined ? { lineOffset: offset } : {}),
    });
    select({ key: target });
  };
  const header = model?.header;
  const missing = query.isError && isMissing(query.error);
  return (
    <article
      className={`board-card entrance-card${selected ? " selected" : ""}${folded ? " folded" : ""}${missing ? " placeholder" : ""}`}
      data-card-id={card.id}
      data-key={key}
      data-loading={query.isPending ? "true" : undefined}
      style={{ width }}
      aria-label={title}
      onClick={() => {
        selectCard(boardId, card.id);
        select({ key });
      }}
      onContextMenu={menu.onContextMenu}
    >
      <TargetHandle />
      <header className="card-header ecard-header card-drag" onDoubleClick={toggleFolded}>
        <FoldToggle folded={folded} label={title} onToggle={toggleFolded} />
        <span className="ecard-kind">{(header?.kind ?? "entrance").toUpperCase()}</span>
        {header?.method ? <span className="ecard-method">{header.method}</span> : null}
        <h3 className="ecard-title" title={title}>
          {header?.trigger ?? title}
        </h3>
        <span className="card-header-spacer" />
        <HeaderPorts ids={folded ? ports : []} />
      </header>
      {folded ? null : (
        <>
          <DescriptionLine description={card.description} />
          <div className="card-body nodrag nopan">
            {query.isPending ? <p className="card-note">Loading…</p> : null}
            {query.isError ? (
              <p className="card-note warn">
                <AlertTriangle size={13} aria-hidden="true" />
                {missing ? "This entrance is missing from the graph." : query.error.message}
              </p>
            ) : null}
            {model ? (
              <>
                {header?.label ? <p className="ecard-label">{header.label}</p> : null}
                <Evidence model={model} />
                <div className="ecard-targets-heading">Targets, in registration order</div>
                {model.targets.length === 0 ? <p className="card-note">No targets resolved.</p> : null}
                <ul className="ecard-targets">
                  {model.targets.map((target) => {
                    const handle = targetHandle(target.position);
                    return <TargetRow key={handle} target={target} port={portSet.has(handle) ? handle : null} onFollow={follow} />;
                  })}
                </ul>
                {model.applications.length > 0 ? (
                  <p className="ecard-apps">Part of {model.applications.join(", ")}</p>
                ) : null}
              </>
            ) : null}
          </div>
        </>
      )}
      {menu.element}
    </article>
  );
}
