/** How it starts: how an application card names its launch and its overflow. */

import type { ApplicationEntrances, LaunchRow } from "../../ipc";
import { formatCount } from "../../lib/format";

/** The command that starts an application, as typed: "npm run start", "orders", "python -m jobs". */
export function launchCommand(launch: Pick<LaunchRow, "detector" | "trigger" | "startup" | "title">): string {
  if (launch.detector === "package.script" && launch.trigger !== null) {
    return `npm run ${launch.trigger}`;
  }
  return launch.trigger ?? launch.startup ?? launch.title;
}

/** "8 more routes" (every entrance an HTTP route) or "8 more entrances". */
export function overflowWords(application: Pick<ApplicationEntrances, "total" | "entrances">): string {
  const more = application.total - application.entrances.length;
  const routes = application.entrances.every((row) => row.kind === "http");
  return `${formatCount(more)} more ${routes ? (more === 1 ? "route" : "routes") : more === 1 ? "entrance" : "entrances"}`;
}

/** The application that grows 1.4 in its row: the one with the most entrances, when there are several. */
export function widestApplication(applications: readonly Pick<ApplicationEntrances, "startup" | "total">[]): string | null {
  if (applications.length < 2) {
    return null;
  }
  return applications.reduce((widest, application) => (application.total > widest.total ? application : widest)).startup;
}

/** At most this many application cards share a row. */
const PER_ROW = 3;

/** The applications in rows of at most three, as even as they go: 4 → 2 + 2, 5 → 3 + 2. */
export function applicationRows<T>(applications: readonly T[]): T[][] {
  const count = Math.ceil(applications.length / PER_ROW);
  const rows: T[][] = [];
  let start = 0;
  for (let row = 0; row < count; row += 1) {
    const size = Math.floor(applications.length / count) + (row < applications.length % count ? 1 : 0);
    rows.push(applications.slice(start, start + size));
    start += size;
  }
  return rows;
}
