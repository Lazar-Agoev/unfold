/**
 * Selected for a resource card (design §4.5): its name, its kind and who drew it, its description with
 * what needs review about it (and Review, its repairs), the type a table is backed by (opening its
 * definition in place), what it cites (each opening its source), the connections in and out of it,
 * and Remove resource card.
 */

import { Author } from "../../agent";
import { ReviewButton } from "../../canvas/cards/ReviewBanner";
import { RESOURCE_ICONS, RESOURCE_WORDS } from "../../canvas/resources/kinds";
import { resourceDescriptionField } from "../../canvas/resources/ResourceNode";
import { Button } from "../../components";
import { Icon } from "../../icons";
import type { Resource } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import { describeResource, findPage, itemsFor, removeItem, useOpenBoard, type PageTarget } from "../../state/boards";
import { AuthoredField } from "./AuthoredField";
import { AnchorRows, LineRows, TypeRow } from "./ClaimRows";

export function SelectedResource({ target, resource }: { readonly target: PageTarget; readonly resource: Resource }) {
  const author = useAuthor();
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  const health = useOpenBoard(target.board, (board) => board.health);
  if (!page) {
    return null;
  }
  const issues = itemsFor(health, target.page, { resource: resource.id });
  const { project } = target.board;
  return (
    <>
      <div className="selected-identity">
        <h3 className="selected-name">{resource.name}</h3>
        <p className="selected-location selected-kind">
          <Icon name={RESOURCE_ICONS[resource.kind]} size={12} />
          {RESOURCE_WORDS[resource.kind]}
          {" · drawn by "}
          <Author author={resource.author} />
        </p>
      </div>
      <AuthoredField
        label="Resource card description"
        value={resource.description}
        placeholder="What it holds or does, in one sentence…"
        focusKey={resourceDescriptionField(resource.id)}
        disabledReason={author === null ? "Loading who you are…" : null}
        status={
          issues.length > 0 ? (
            <span className="authored-note">
              {issues.map((item) => item.message).join(" ")} <ReviewButton target={target} project={project} items={issues} />
            </span>
          ) : null
        }
        onChange={(text) => {
          if (author !== null) {
            describeResource(target, resource.id, text, author);
          }
        }}
      />
      <div className="selected-relations">
        {resource.type !== undefined ? <TypeRow label="Type" project={project} typeKey={resource.type} /> : null}
        <AnchorRows target={target} anchors={resource.anchors} />
        <LineRows target={target} page={page} id={resource.id} />
      </div>
      <div className="selected-actions">
        <Button
          variant="secondary"
          onClick={() => {
            removeItem(target, resource.id);
          }}
        >
          Remove resource card
        </Button>
      </div>
    </>
  );
}
