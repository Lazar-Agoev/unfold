/**
 * A table card's columns (design §4.6). A table backed by a type lists the type's fields, compiled:
 * its own properties, then those it inherits, base by base (`type_definition`'s members and inherited
 * groups), each with its written type and whether it is optional; the board's columns only lay marks
 * on them (primary key, foreign key with what it references, unique). A table that lists its own
 * columns shows them as written. A card lists up to 12; the rest are behind "N more". Pure.
 */

import type { Column, ColumnKey, Resource, TypeDefinition, TypeMember, TypeText } from "../../ipc/generated";

/** One column as a table card shows it. */
export interface TableColumn {
  readonly name: string;
  /** Its written type: the type's field's (`TypeText`), or the one the board wrote for its own column. */
  readonly type?: TypeText | string;
  readonly optional: boolean;
  readonly key?: ColumnKey;
  /** What a foreign key references (`customers.id`). */
  readonly references?: string;
  /** The base it is inherited from, as written (`OrderDraft`), for a field of a base. */
  readonly from?: string;
}

/** A table card lists this many columns at most before "N more". */
export const COLUMNS_LISTED = 12;

/** The properties of a type's members: the fields a table's columns are. */
function properties(members: readonly TypeMember[]): (TypeMember & { readonly name: string })[] {
  return members.filter((member): member is TypeMember & { readonly name: string } => member.kind === "property" && member.name !== undefined);
}

/** A column's marks, from the board's column of that name. */
function marks(column: Column | undefined): Pick<TableColumn, "key" | "references"> {
  return {
    ...(column?.key !== undefined ? { key: column.key } : {}),
    ...(column?.references !== undefined ? { references: column.references } : {}),
  };
}

/**
 * A table's columns: its type's fields with the board's marks laid on (`definition`, once loaded; null
 * until then), or its own. Not a table, or a type not loaded yet: none.
 */
export function tableColumns(resource: Pick<Resource, "kind" | "type" | "columns">, definition: TypeDefinition | null | undefined): TableColumn[] {
  if (resource.kind !== "table") {
    return [];
  }
  const written = resource.columns ?? [];
  if (resource.type === undefined) {
    return written.map((column) => ({
      name: column.name,
      ...(column.type !== undefined ? { type: column.type } : {}),
      optional: column.optional === true,
      ...marks(column),
    }));
  }
  if (!definition) {
    return [];
  }
  const byName = new Map(written.map((column) => [column.name, column]));
  const field = (member: TypeMember & { readonly name: string }, from?: string): TableColumn => ({
    name: member.name,
    ...(member.type ? { type: member.type } : {}),
    optional: member.optional,
    ...marks(byName.get(member.name)),
    ...(from !== undefined ? { from } : {}),
  });
  return [
    ...properties(definition.members).map((member) => field(member)),
    ...definition.inherited.flatMap((group) => properties(group.members).map((member) => field(member, group.from.text))),
  ];
}

/** The written type of a column as plain text. */
export function columnTypeText(column: TableColumn): string {
  return typeof column.type === "string" ? column.type : (column.type?.text ?? "");
}

/** A column's marks as the card writes them: `PK`, `FK → customers.id`, `unique`. */
export function markText(column: Pick<TableColumn, "key" | "references">): string {
  switch (column.key) {
    case "primary":
      return "PK";
    case "foreign":
      return column.references !== undefined ? `FK → ${column.references}` : "FK";
    case "unique":
      return "unique";
    default:
      return "";
  }
}

/** The columns a card lists (up to 12) and how many "N more" hides. */
export function listedColumns(columns: readonly TableColumn[], all: boolean): { readonly shown: readonly TableColumn[]; readonly hidden: number } {
  const shown = all || columns.length <= COLUMNS_LISTED ? columns : columns.slice(0, COLUMNS_LISTED);
  return { shown, hidden: columns.length - shown.length };
}

/** The handle a relationship leaves (`out`) or enters (`in`) a table's column by, on its `left` or `right` edge. */
export function columnHandle(column: string, way: "in" | "out", side: "left" | "right"): string {
  return `col-${way}-${side}:${column}`;
}
