/**
 * Selected for a connection, apart from compiled relations: its kind (changed from a menu, which makes
 * it yours), who drew it, its label and description (edited in place) with what needs review about it
 * (and Review, its repairs), its ends, the call it leaves from (as written, with its line), a
 * relationship's columns and cardinality (picked from the two tables' columns), its channel, the type
 * it carries, what it cites, and Remove connection.
 */

import { useId, useRef } from "react";
import { Author } from "../../agent";
import { LINK_DESCRIPTION_FIELD, LINK_LABEL_FIELD, kindEntries } from "../../canvas/connections/linkMenu";
import { ReviewButton } from "../../canvas/cards/ReviewBanner";
import type { TableColumn } from "../../canvas/resources/columns";
import { useColumns } from "../../canvas/resources/useColumns";
import { Button, MenuButton, showToast, type MenuEntry } from "../../components";
import { useEngineQuery } from "../../ipc";
import type { Cardinality, Connection } from "../../ipc/generated";
import { useFocusTarget } from "../../lib/focus-targets";
import { useAuthor } from "../../state/author";
import {
  CONNECTION_WORDS,
  describeLink,
  findPage,
  findResource,
  itemsFor,
  removeLink,
  setLinkLabel,
  setRelationship,
  useOpenBoard,
  type PageTarget,
  type RelationshipChange,
} from "../../state/boards";
import { useCardNames } from "../../state/cards";
import { AuthoredField } from "./AuthoredField";
import { AnchorRows, TypeRow, itemName, selectItem } from "./ClaimRows";
import { RelationRow } from "./Relations";

/** The call a connection leaves from: as written, and its line. */
function StepRow({ target, step }: { readonly target: PageTarget; readonly step: string }) {
  const contract = useEngineQuery("call_contract", { project: target.board.project, key: step });
  return (
    <RelationRow label="Step" title={step}>
      <span className="relation-code">{contract.data ? `${contract.data.text} · line ${contract.data.line}` : "…"}</span>
    </RelationRow>
  );
}

/** The connection's label, edited in place. */
function LabelRow({ target, link, author }: { readonly target: PageTarget; readonly link: Connection; readonly author: string | null }) {
  const field = useRef<HTMLInputElement>(null);
  const id = useId();
  useFocusTarget(LINK_LABEL_FIELD, field);
  return (
    <div className="relation-row">
      <label className="relation-label" htmlFor={id}>
        Label
      </label>
      <input
        ref={field}
        id={id}
        className="relation-input"
        value={link.label?.text ?? ""}
        placeholder={link.kind === "other" ? "What it is, in a few words…" : "What travels along it…"}
        disabled={author === null}
        onChange={(event) => {
          if (author !== null) {
            setLinkLabel(target, link.id, event.target.value, author);
          }
        }}
      />
    </div>
  );
}

/** Each cardinality in words, as the menu lists them. */
const CARDINALITIES: readonly { readonly value: Cardinality; readonly text: string }[] = [
  { value: "one-to-one", text: "One to one" },
  { value: "one-to-many", text: "One to many" },
  { value: "many-to-one", text: "Many to one" },
  { value: "many-to-many", text: "Many to many" },
];

/** A column picker: the columns of one table, the chosen one checked. */
function columnEntries(columns: readonly TableColumn[], chosen: string | undefined, onPick: (name: string) => void): MenuEntry[] {
  return columns.map((column) => ({
    id: `column-${column.name}`,
    label: column.name,
    checked: column.name === chosen,
    onSelect: () => {
      onPick(column.name);
    },
  }));
}

/**
 * A relationship's columns and cardinality: which column of the
 * table it starts at holds which column of the other, and how many of each. Each change is saved at
 * once as one undo step and makes you the one who drew it; what the board refuses comes back as a
 * message.
 */
function RelationshipRows({ target, link, author }: { readonly target: PageTarget; readonly link: Connection; readonly author: string | null }) {
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  const from = page ? findResource(page, link.from) : undefined;
  const to = page ? findResource(page, link.to) : undefined;
  const fromColumns = useColumns(target.board.project, from).columns;
  const toColumns = useColumns(target.board.project, to).columns;
  const change = (next: RelationshipChange) => {
    if (author === null) {
      return;
    }
    void setRelationship(target, link.id, next, author).then((outcome) => {
      if (!outcome.ok) {
        showToast({ title: "Couldn't change the relationship", body: outcome.message, tone: "danger" });
      }
    });
  };
  const cardinality = CARDINALITIES.find((item) => item.value === link.cardinality);
  return (
    <>
      <div className="relation-row">
        <span className="relation-label">Columns</span>
        <span className="relation-columns">
          <MenuButton
            variant="text"
            size="sm"
            label={link.fromColumn ?? "Column"}
            menuLabel={`Column of ${from?.name ?? "the first table"}`}
            entries={columnEntries(fromColumns, link.fromColumn, (name) => {
              change({ fromColumn: name });
            })}
          />
          <span aria-hidden="true">→</span>
          <MenuButton
            variant="text"
            size="sm"
            label={link.toColumn ?? "Column"}
            menuLabel={`Column of ${to?.name ?? "the second table"}`}
            entries={columnEntries(toColumns, link.toColumn, (name) => {
              change({ toColumn: name });
            })}
          />
        </span>
      </div>
      <div className="relation-row">
        <span className="relation-label">Cardinality</span>
        <MenuButton
          variant="text"
          size="sm"
          label={cardinality?.text ?? "Choose"}
          menuLabel="Cardinality"
          entries={CARDINALITIES.map((item) => ({
            id: `cardinality-${item.value}`,
            label: item.text,
            checked: item.value === link.cardinality,
            onSelect: () => {
              change({ cardinality: item.value });
            },
          }))}
        />
      </div>
    </>
  );
}

export function SelectedConnection({ target, link }: { readonly target: PageTarget; readonly link: Connection }) {
  const author = useAuthor();
  const names = useCardNames(target);
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  const health = useOpenBoard(target.board, (board) => board.health);
  if (!page) {
    return null;
  }
  const issues = itemsFor(health, target.page, { connection: link.id });
  const end = (label: string, id: string) => (
    <RelationRow
      label={label}
      onActivate={() => {
        selectItem(target, page, id);
      }}
    >
      <span className="relation-code">{itemName(page, id, names)}</span>
    </RelationRow>
  );
  return (
    <>
      <div className="selected-link-identity">
        <h3 className="selected-name ui">{CONNECTION_WORDS[link.kind]} connection</h3>
        <span className="selected-byline">
          · drawn by <Author author={link.author} />
        </span>
      </div>
      <div className="selected-relations link">
        <div className="relation-row">
          <span className="relation-label">Kind</span>
          <MenuButton variant="text" size="sm" label={CONNECTION_WORDS[link.kind]} menuLabel="Connection kind" entries={kindEntries(target, link.id, author)} />
        </div>
        <LabelRow target={target} link={link} author={author} />
        {end("From", link.from)}
        {end("To", link.to)}
        {link.step !== undefined ? <StepRow target={target} step={link.step} /> : null}
        {link.kind === "relates" ? <RelationshipRows target={target} link={link} author={author} /> : null}
        {link.channel !== undefined ? (
          <RelationRow label="Channel">
            <span className="relation-code">{link.channel}</span>
          </RelationRow>
        ) : null}
        {link.carries !== undefined ? <TypeRow project={target.board.project} label="Carries" typeKey={link.carries} /> : null}
        <AnchorRows target={target} anchors={link.anchors ?? []} />
      </div>
      <AuthoredField
        label="Connection description"
        value={link.description}
        placeholder="The evidence, in words…"
        focusKey={LINK_DESCRIPTION_FIELD}
        disabledReason={author === null ? "Loading who you are…" : null}
        status={
          issues.length > 0 ? (
            <span className="authored-note">
              {issues.map((item) => item.message).join(" ")} <ReviewButton target={target} project={target.board.project} items={issues} />
            </span>
          ) : null
        }
        onChange={(text) => {
          if (author !== null) {
            describeLink(target, link.id, text, author);
          }
        }}
      />
      <p className="selected-note">Drawn, not compiled: Called by, Reached from, and the cards list only what the code proves.</p>
      <div className="selected-actions">
        <Button
          variant="secondary"
          onClick={() => {
            removeLink(target, link.id);
          }}
        >
          Remove connection
        </Button>
      </div>
    </>
  );
}
