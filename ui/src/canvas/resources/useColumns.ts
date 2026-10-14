/** A table card's columns as a hook: from its type's definition (once loaded), or as it lists them. */

import { useMemo } from "react";
import type { ProjectId, Resource } from "../../ipc/generated";
import { useEngineQuery } from "../../ipc/queries";
import { tableColumns, type TableColumn } from "./columns";

/** A table's columns, from its type's definition (once loaded) or as it lists them, and whether its type is still loading. */
export function useColumns(project: ProjectId, resource: Resource | undefined): { readonly columns: TableColumn[]; readonly loading: boolean } {
  const typeKey = resource?.kind === "table" ? resource.type : undefined;
  const wanted = typeKey !== undefined;
  const query = useEngineQuery("type_definition", { project, key: typeKey ?? "" }, { enabled: wanted });
  const columns = useMemo(() => (resource ? tableColumns(resource, query.data) : []), [resource, query.data]);
  return { columns, loading: wanted && query.isPending };
}
