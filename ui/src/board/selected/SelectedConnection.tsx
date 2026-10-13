/**
 * Selected for a connection (design §4.7), apart from compiled relations: its kind (changed from a
 * menu, which makes it yours), who drew it, its label and description (edited in place) with what
 * needs review about it (and Review, its repairs), its ends, the step it leaves from (`file:line`,
 * opening the source), a relationship's columns and cardinality, its channel, the type it carries
 * (opening its definition in place), what it cites, and Remove connection.
 */

import { useQuery } from "@tanstack/react-query";
import { useId, useRef } from "react";
import { Author } from "../../agent";
import { cardQuery } from "../../canvas";
import { cardinalityText } from "../../canvas/connections/chip";
import { LINK_DESCRIPTION_FIELD, MENU_WORDS, kindEntries } from "../../canvas/connections/linkMenu";
import { ReviewButton } from "../../canvas/cards/ReviewBanner";
import { LINK_LABEL_FIELD } from "../../canvas/cards/select";
import { portSpot } from "../../code/CodeView";
import { stepPort } from "../../code/outline";
import { shortPath } from "../../code/text";
import { Button, MenuButton } from "../../components";
import type { Link, Page, ProjectId } from "../../ipc/generated";
import { useFocusTarget } from "../../lib/focus-targets";
import { useAuthor } from "../../state/author";
import { describeLink, findCard, findPage, itemsFor, removeLink, setLinkLabel, useOpenBoard, type PageTarget } from "../../state/boards";
import { openPeek } from "../../state/overlays";
import { resolveRefProject, useProjectIndex } from "../../state/workspace";
import { AuthoredField } from "./AuthoredField";
import { AnchorRows, TypeRow, itemName, selectItem } from "./ClaimRows";
import { RelationRow } from "./Relations";

/** The step a connection leaves from, as `file:line` opening its source (its card's Code model reads where it is). */
function StepRow({ target, page, link, step }: { readonly target: PageTarget; readonly page: Page; readonly link: Link; readonly step: string }) {
  const card = findCard(page, link.from);
  const project = resolveRefProject(useProjectIndex(), target.board.project, card?.ref.project);
  const key = card && "block" in card.ref ? card.ref.block : "";
  const query = useQuery({ ...cardQuery(project ?? "", key), enabled: project !== null && key.length > 0 });
  const model = query.data?.type === "block" ? query.data : undefined;
  const code = model?.code?.source.state === "current" ? model.code.source : undefined;
  const spot = code ? portSpot(code, stepPort(step)) : undefined;
  const place = model && spot ? { path: model.header.path, line: spot.line } : null;
  return (
    <RelationRow
      label="Step"
      title={step}
      {...(place && project !== null
        ? {
            onActivate: () => {
              openPeek({ project, path: place.path, line: place.line });
            },
          }
        : {})}
    >
      <span className="relation-code">{place ? `${shortPath(place.path)}:${place.line}` : step}</span>
    </RelationRow>
  );
}

/** The connection's label, edited in place. */
function LabelRow({ target, link, author }: { readonly target: PageTarget; readonly link: Link; readonly author: string | null }) {
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

export function SelectedConnection({ target, link }: { readonly target: PageTarget; readonly link: Link & { readonly kind: NonNullable<Link["kind"]> } }) {
  const author = useAuthor();
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  const health = useOpenBoard(target.board, (board) => board.health);
  const index = useProjectIndex();
  if (!page) {
    return null;
  }
  const issues = itemsFor(health, target.page, { link: link.id });
  const carriesProject: ProjectId | null = link.carries !== undefined ? resolveRefProject(index, target.board.project, link.carriesProject) : null;
  const end = (label: string, id: string) => (
    <RelationRow
      label={label}
      onActivate={() => {
        selectItem(target, page, id);
      }}
    >
      <span className="relation-code">{itemName(page, id)}</span>
    </RelationRow>
  );
  return (
    <>
      <div className="selected-link-identity">
        <h3 className="selected-name ui">{MENU_WORDS[link.kind]} connection</h3>
        {link.author !== undefined ? (
          <span className="selected-byline">
            · drawn by <Author author={link.author} />
          </span>
        ) : null}
      </div>
      <div className="selected-relations link">
        <div className="relation-row">
          <span className="relation-label">Kind</span>
          <MenuButton variant="text" size="sm" label={MENU_WORDS[link.kind]} menuLabel="Connection kind" entries={kindEntries(target, link.id, author)} />
        </div>
        <LabelRow target={target} link={link} author={author} />
        {end("From", link.from)}
        {end("To", link.to)}
        {link.step !== undefined ? <StepRow target={target} page={page} link={link} step={link.step} /> : null}
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
        {link.carries !== undefined ? <TypeRow label="Carries" project={carriesProject} typeKey={link.carries} /> : null}
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
      <p className="selected-note">Drawn, not compiled: Called by, Reached from, and Flow list only what the code proves.</p>
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
