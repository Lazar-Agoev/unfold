/** How it starts in words: the summary line, the setup card's sentences, and a scope's overflow. */

import type { CatalogCounts, CatalogRow, DeployImport, LaunchSuggestions, ServiceType } from "../../ipc";
import { formatCount, plural } from "../../lib/format";

/** `shared`, `shared and api`, `shared, api, and utils`. */
export function listWords(words: readonly string[]): string {
  if (words.length < 3) {
    return words.join(" and ");
  }
  return `${words.slice(0, -1).join(", ")}, and ${words.at(-1) ?? ""}`;
}

/** The line beside the section's title, from the catalog's totals. */
export function summaryLine(counts: CatalogCounts, compiling: boolean): string {
  if (counts.launches > 0) {
    return `${plural(counts.applications, "application")} · ${plural(counts.launches, "launch", "launches")} · ${plural(counts.routes, "route")}. Hover a row to trace it on a new board.`;
  }
  if (counts.routes > 0) {
    return `${plural(counts.routes, "route")}, but no launches yet, so Unfold can't tell which service serves them.`;
  }
  return compiling ? "Entrances show up here as the compile finds them." : "No launches or routes yet.";
}

/**
 * Why a project without launches reads the way it does, with only the parts that apply:
 * "There are no package scripts or pyproject entry points here, so routes belong to no application,
 * and imports like shared, api, and utils read as packages: 249 of 7,239 imports resolve."
 */
export function setupSentence(suggestions: Pick<LaunchSuggestions, "routes" | "imports" | "localLooking"> | undefined): string {
  const consequences: string[] = [];
  if (suggestions && suggestions.routes > 0) {
    consequences.push("routes belong to no application");
  }
  const { imports, localLooking = [] } = suggestions ?? {};
  const resolve = imports && imports.resolved < imports.total ? `${formatCount(imports.resolved)} of ${formatCount(imports.total)} imports resolve` : null;
  if (localLooking.length > 0) {
    consequences.push(`imports like ${listWords(localLooking)} read as packages${resolve === null ? "" : `: ${resolve}`}`);
  } else if (resolve !== null) {
    consequences.push(`only ${resolve}`);
  }
  const lead = "There are no package scripts or pyproject entry points here";
  return consequences.length === 0 ? `${lead}.` : `${lead}, so ${consequences.join(", and ")}.`;
}

const SERVICE_WORDS: Readonly<Record<ServiceType, readonly [string, string]>> = {
  web: ["web service", "web services"],
  worker: ["worker", "workers"],
  cron: ["cron job", "cron jobs"],
};

/** "5 web services, 3 workers, and 8 cron jobs in render.yaml and data-pipelines/render.yaml". */
export function deployBreakdown(deploy: DeployImport): string {
  const kinds = (Object.keys(SERVICE_WORDS) as ServiceType[]).flatMap((type) => {
    const count = deploy.launches.filter((launch) => launch.serviceType === type).length;
    const [one, many] = SERVICE_WORDS[type];
    return count === 0 ? [] : [plural(count, one, many)];
  });
  return `${listWords(kinds)} in ${listWords(deploy.files)}`;
}

/** "2 more routes" when every row shown past the launches is a route, else "2 more entrances". */
export function moreWords(more: number, shown: readonly Pick<CatalogRow, "kind">[]): string {
  const routes = shown.every((row) => row.kind === "http" || row.kind === "process");
  return routes ? `${formatCount(more)} more ${more === 1 ? "route" : "routes"}` : `${formatCount(more)} more ${more === 1 ? "entrance" : "entrances"}`;
}
