/** Who signs this window's board edits: `person:<username>`, from `app_info`. */

import { useEngineQuery } from "../ipc/queries";

/** The author of new descriptions and labels, or null until `app_info` answers. */
export function useAuthor(): string | null {
  const info = useEngineQuery("app_info", {});
  const username = info.data?.username.trim();
  return username !== undefined && username.length > 0 ? `person:${username}` : null;
}
