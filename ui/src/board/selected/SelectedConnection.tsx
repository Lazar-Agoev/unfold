/**
 * Selected for a connection, apart from compiled relations: its kind (changed from a menu, which makes
 * it yours), who drew it, its label and description (edited in place) with what needs review about it
 * (and Review, its repairs), its ends, the call it leaves from (as written, with its line), a
 * relationship's columns and cardinality, its channel, the type it carries, what it cites, and Remove
 * connection.
 */

import { useId, useRef } from "react";
import { Author } from "../../agent";
import { cardinalityText } from "../../canvas/connections/chip";
import { LINK_DESCRIPTION_FIELD, LINK_LABEL_FIELD, kindEntries } from "../../canvas/connections/linkMenu";
import { ReviewButton } from "../../canvas/cards/ReviewBanner";
import { Button, MenuButton } from "../../components";
import { useEngineQuery } from "../../ipc";
import type { Connection } from "../../ipc/generated";
import { useFocusTarget } from "../../lib/focus-targets";
import { useAuthor } from "../../state/author";
import { CONNECTION_WORDS, describeLink, findPage, itemsFor, removeLink, setLinkLabel, useOpenBoard, type PageTarget } from "../../state/boards";
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
        {link.kind === "relates" && (link.fromColumn !== undefined || link.toColumn !== undefined) ? (
          <RelationRow label="Columns">
            <span className="relation-code">{`${link.fromColumn ?? "?"} → ${link.toColumn ?? "?"}`}</span>
          </RelationRow>
        ) : null}
        {link.cardinality !== undefined ? (
          <RelationRow label="Cardinality">
            <span className="relation-code">{`${cardinalityText(link.cardinality)} (${link.cardinality.replaceAll("-", " ")})`}</span>
          </RelationRow>
        ) : null}
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
