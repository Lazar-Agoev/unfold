/**
 * Page revisions: what an agent passes as `expectRevision`, so an edit is refused when the page
 * changed since the agent read it. Computed only here, from the page JSON a window or the engine
 * reports.
 */

const FNV_OFFSET = 0xcbf29ce484222325n;
const FNV_PRIME = 0x100000001b3n;
const MASK_64 = 0xffffffffffffffffn;

/** `value` with every object's keys sorted, so equal pages serialize to equal text. */
function sortedKeys(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(sortedKeys);
  }
  if (typeof value === "object" && value !== null) {
    const record = value as Record<string, unknown>;
    return Object.fromEntries(
      Object.keys(record)
        .sort()
        .map((key) => [key, sortedKeys(record[key])]),
    );
  }
  return value;
}

/**
 * A page's revision: the first 8 hex digits of FNV-1a 64 over its JSON as the board file writes it
 * (keys sorted, two-space indent), as UTF-8.
 */
export function pageRevision(page: unknown): string {
  let hash = FNV_OFFSET;
  for (const byte of new TextEncoder().encode(JSON.stringify(sortedKeys(page), null, 2))) {
    hash = ((hash ^ BigInt(byte)) * FNV_PRIME) & MASK_64;
  }
  return hash.toString(16).padStart(16, "0").slice(0, 8);
}
