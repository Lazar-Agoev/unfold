/**
 * The Index log (architecture §11.5): each committed update, newest first, with the engine's facts
 * (files read, taken from cache, or failed; time per stage; counts by resolution, reason, entrance
 * kind, and detector), and the committed graph's counts.
 */

import { EmptyState } from "../components";
import type { GraphStats, IndexLogEntry } from "../ipc/generated";
import { useEngineQuery } from "../ipc/queries";
import { plural } from "../shell/status";
import "./panel.css";

/** The engine's fact lines as label/value pairs; the first line is the entry's summary. */
export function factLines(text: string): { summary: string | null; facts: { label: string; value: string }[] } {
  const lines = text.split("\n").filter((line) => line.trim() !== "");
  const [summary, ...rest] = lines;
  return {
    summary: summary ?? null,
    facts: rest.map((line) => {
      const colon = line.indexOf(": ");
      return colon < 0 ? { label: "", value: line } : { label: line.slice(0, colon), value: line.slice(colon + 2) };
    }),
  };
}

function clock(atMs: number): string {
  return new Date(atMs).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

function bytes(count: number): string {
  if (count < 1024 * 1024) {
    return `${Math.max(1, Math.round(count / 1024)).toLocaleString("en-US")} KB`;
  }
  return `${(count / (1024 * 1024)).toFixed(1)} MB`;
}

function Stats({ stats }: { readonly stats: GraphStats }) {
  const rows: [string, string][] = [
    [
      "Files",
      [
        plural(stats.filesCurrent, "current file"),
        stats.filesLastGood > 0 ? `${stats.filesLastGood.toLocaleString("en-US")} showing their last version that parsed` : null,
        stats.filesFailed > 0 ? `${stats.filesFailed.toLocaleString("en-US")} failed` : null,
        plural(stats.inputs, "input"),
      ]
        .filter(Boolean)
        .join(" · "),
    ],
    ["Blocks", `${plural(stats.blocks, "block")} · ${plural(stats.definitions, "definition")}`],
    [
      "Edges",
      `${stats.edgesResolved.toLocaleString("en-US")} resolved · ${stats.edgesExternal.toLocaleString("en-US")} external · ${stats.edgesUnresolved.toLocaleString("en-US")} unresolved`,
    ],
    ["Entrances", `${plural(stats.entrances, "entrance")} · ${plural(stats.applications, "application")}`],
    ["Database", bytes(stats.databaseBytes)],
  ];
  return (
    <dl className="index-facts index-stats" aria-label="Graph counts">
      {rows.map(([label, value]) => (
        <div key={label} className="index-fact">
          <dt>{label}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function Entry({ entry }: { readonly entry: IndexLogEntry }) {
  const { facts } = factLines(entry.text);
  return (
    <article className="index-entry">
      <header className="index-entry-header">
        <span className="index-entry-action">{entry.action}</span>
        <span className="index-entry-meta">
          {clock(entry.atMs)} · {plural(entry.files, "file")} · update {entry.updateMs.toLocaleString("en-US")} ms · commit{" "}
          {entry.commitMs.toLocaleString("en-US")} ms
        </span>
      </header>
      <dl className="index-facts">
        {facts.map((fact, index) => (
          <div key={index} className="index-fact">
            <dt>{fact.label}</dt>
            <dd>{fact.value}</dd>
          </div>
        ))}
      </dl>
    </article>
  );
}

export function IndexLogPanel() {
  const query = useEngineQuery("index_log", {});
  if (query.error) {
    return <p className="panel-hint panel-error">{query.error.message}</p>;
  }
  if (!query.data) {
    return <p className="panel-hint">Loading…</p>;
  }
  const { entries, stats } = query.data;
  if (entries.length === 0 && !stats) {
    return <EmptyState size="compact" title="Nothing compiled yet" hint="Each compile and background update is logged here." />;
  }
  return (
    <div className="index-log">
      {stats ? <Stats stats={stats} /> : null}
      {[...entries].reverse().map((entry) => (
        <Entry key={`${entry.atMs}-${entry.action}`} entry={entry} />
      ))}
    </div>
  );
}
