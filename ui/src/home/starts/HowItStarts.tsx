/**
 * How it starts on Home (entrance-definitions.md, "Home: How it starts"; entrance-definitions.html): one
 * quiet list in two columns, names on the left and `run` or `on` in mono on the right. Apps are the bold
 * rows, with their entrances indented under them, in file order. Hover shows a chevron, and a copy icon
 * beside an app's command. A problem is an amber dot before the name; hovering the row says what
 * happened in the right column, with Fix when there is one. Clicking a row traces it onto a new board;
 * a row the graph left out opens its lines in `unfold.toml`. Search appears once the list has more than
 * 15 entrances. With no apps: "No entrances yet", Add entrance, and Copy agent prompt.
 */

import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { Button, EmptyState, IconButton, Popover, SearchField, showToast } from "../../components";
import { AddEntrance, CodeSearch } from "../../entrances";
import { Icon } from "../../icons";
import { toUnfoldError, useEngineQuery, windowCommand, type ProjectId } from "../../ipc";
import { call } from "../../ipc/commands";
import type { Key } from "../../ipc/generated";
import { openInEditor } from "../../lib/files";
import { AGENT_PROMPT, SEARCH_THRESHOLD, entranceCount, filterRows, fixOf, rewriteCode, startRows, type CodeSlot, type RowFix, type RowProblem, type StartRow } from "./model";
import { traceStart } from "./trace";
import "./starts.css";

/** How long a copied command shows its check, and How it starts' problems their highlight. */
const COPIED_MS = 1200;
const HIGHLIGHT_MS = 1600;

/** The settings file, where `[scope]` and the `[[app]]` tables live. */
const CONFIG = "unfold.toml";

/** What starts an entrance, in mono; an HTTP method before a path in bold, as the page draws it. */
function Command({ text }: { readonly text: string }) {
  const method = /^(GET|HEAD|POST|PUT|PATCH|DELETE|OPTIONS) /.exec(text)?.[1];
  return (
    <span className="home-start-command">
      {method === undefined ? (
        text
      ) : (
        <>
          <b>{method}</b>
          {text.slice(method.length)}
        </>
      )}
    </span>
  );
}

/** Copies an app's command; a check shows for a moment once it is copied. */
function CopyCommand({ text }: { readonly text: string }) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) {
      return undefined;
    }
    const timer = window.setTimeout(() => {
      setCopied(false);
    }, COPIED_MS);
    return () => {
      window.clearTimeout(timer);
    };
  }, [copied]);
  return (
    <IconButton
      icon={copied ? "check" : "copy"}
      label="Copy command"
      size={20}
      className="home-start-copy"
      onClick={(event) => {
        event.stopPropagation();
        windowCommand("copy_to_clipboard", { text }).then(
          () => {
            setCopied(true);
          },
          (error: unknown) => {
            showToast({ title: "Couldn't copy the command", body: toUnfoldError(error).message, tone: "danger" });
          },
        );
      }}
    />
  );
}

/** What happened, in the right column of a row with a problem. */
function explanation(problem: RowProblem): string {
  switch (problem.kind) {
    case "entry":
      return problem.message;
    case "missing":
      if (problem.fix === undefined) {
        return "Not found";
      }
      return problem.fix.kind === "edited_route" ? "Route edited" : `Moved to ${problem.fix.path}`;
    case "not_analyzed":
      return "Not analyzed";
    case "not_a_start":
      return "Not a start";
  }
}

interface RowProps {
  readonly project: ProjectId;
  readonly row: StartRow;
  readonly highlight: boolean;
  /** Opens Choose code… for a code key, beside the row. */
  readonly onChoose: (slot: CodeSlot, anchor: HTMLElement) => void;
}

/** What happened, and the problem's Fix. */
function Problem({ problem, onFix }: { readonly problem: RowProblem; readonly onFix: (fix: RowFix) => void }) {
  const fix = fixOf(problem);
  return (
    <span className="home-start-problem">
      <span className="home-start-why">{explanation(problem)}</span>
      {fix ? (
        <button
          type="button"
          className="home-start-fix"
          onClick={(event) => {
            event.stopPropagation();
            onFix(fix);
          }}
        >
          {fix.kind === "choose" ? "Choose code…" : "Fix"}
        </button>
      ) : null}
    </span>
  );
}

function Row({ project, row, highlight, onChoose }: RowProps) {
  const element = useRef<HTMLDivElement>(null);
  const { action, problem } = row;
  const run = () => {
    if (action?.kind === "trace") {
      void traceStart(project, action.key, row.name);
    } else if (action?.kind === "file") {
      void openInEditor(project, CONFIG, action.line);
    }
  };
  const fix = (chosen: RowFix) => {
    switch (chosen.kind) {
      case "rewrite":
        rewrite(project, chosen.slot, chosen.code);
        return;
      case "choose":
        if (element.current) {
          onChoose(chosen.slot, element.current);
        }
        return;
      case "config":
        void openInEditor(project, CONFIG);
        return;
    }
  };
  // `app-row`, not `app`: `.app` is the window's shell.
  const classes = ["home-start-row", `${row.kind}-row`, problem ? "bad" : "", problem && highlight ? "highlight" : ""].filter(Boolean).join(" ");
  return (
    <div
      ref={element}
      role={action ? "button" : undefined}
      tabIndex={action ? 0 : undefined}
      className={classes}
      onClick={action ? run : undefined}
      onKeyDown={(event: KeyboardEvent<HTMLDivElement>) => {
        if (action && event.key === "Enter" && event.target === event.currentTarget) {
          event.preventDefault();
          run();
        }
      }}
    >
      {problem ? <i className="home-start-dot" role="img" aria-label="Needs a look" /> : null}
      <span className="home-start-name">{row.name}</span>
      <span className="home-start-right">
        {row.command !== undefined ? <Command text={row.command} /> : null}
        {problem ? <Problem problem={problem} onFix={fix} /> : null}
        {row.kind === "app" && row.command !== undefined ? <CopyCommand text={row.command} /> : <span className="home-start-copy-space" />}
      </span>
      <span className="home-start-chevron">{action ? <Icon name="chevron-right" size={12} /> : null}</span>
    </div>
  );
}

/** Copies the request that asks Claude Code or Codex to write the project's `[[app]]` tables. */
function copyAgentPrompt(): void {
  windowCommand("copy_to_clipboard", { text: AGENT_PROMPT }).then(
    () => {
      showToast({ title: "Copied the agent prompt" });
    },
    (error: unknown) => {
      showToast({ title: "Couldn't copy the agent prompt", body: toUnfoldError(error).message, tone: "danger" });
    },
  );
}

/** Writes one edit to `unfold.toml`; `entrances_changed` then refreshes the list. */
function rewrite(project: ProjectId, slot: CodeSlot, code: Key): void {
  const edit = rewriteCode(slot, code);
  if (edit === null) {
    return;
  }
  call("update_project_config", { project, edits: [edit] }).catch((error: unknown) => {
    showToast({ title: "Couldn't change unfold.toml", body: toUnfoldError(error).message, tone: "danger" });
  });
}

/** Choose code…: the name search beside a row; picking a result rewrites that one code key. */
function ChooseCode({ project, slot, anchor, onClose }: { readonly project: ProjectId; readonly slot: CodeSlot; readonly anchor: HTMLElement; readonly onClose: () => void }) {
  const field = useRef<HTMLInputElement>(null);
  return (
    <Popover open anchor={anchor} label="Choose code" align="end" width={400} initialFocus={field} onClose={onClose}>
      <div className="home-choose-code">
        <CodeSearch
          ref={field}
          project={project}
          onPick={(hit) => {
            onClose();
            rewrite(project, slot, hit.key);
          }}
        />
      </div>
    </Popover>
  );
}

export interface HowItStartsProps {
  readonly project: ProjectId;
  /** Changes when the Health line asks to show the problems: the section scrolls into view and they light up. */
  readonly highlight: number;
}

export function HowItStarts({ project, highlight }: HowItStartsProps) {
  const entrances = useEngineQuery("entrances", { project });
  const section = useRef<HTMLElement>(null);
  // The search, as typed since the Health line last asked to show the problems (which clears it).
  const [search, setSearch] = useState({ text: "", highlight });
  const query = search.highlight === highlight ? search.text : "";
  const [adding, setAdding] = useState<HTMLElement | null>(null);
  const [choosing, setChoosing] = useState<{ readonly slot: CodeSlot; readonly anchor: HTMLElement } | null>(null);
  // The problems light up until the highlight asked for has run its time.
  const [shown, setShown] = useState(highlight);
  const lit = highlight !== shown;

  useEffect(() => {
    if (highlight === 0) {
      return undefined;
    }
    section.current?.scrollIntoView({ block: "start", behavior: "smooth" });
    const timer = window.setTimeout(() => {
      setShown(highlight);
    }, HIGHLIGHT_MS);
    return () => {
      window.clearTimeout(timer);
    };
  }, [highlight]);

  const view = entrances.data;
  if (entrances.error) {
    return (
      <section className="home-starts" aria-label="How it starts">
        <h2 className="home-section-title">How it starts</h2>
        <p className="home-starts-note">{entrances.error.message}</p>
      </section>
    );
  }
  if (!view?.resolved) {
    return null;
  }
  const rows = filterRows(startRows(view), query);
  const empty = view.apps.length === 0;
  const add = (event: MouseEvent<HTMLButtonElement>) => {
    setAdding(event.currentTarget);
  };

  return (
    <section ref={section} className="home-starts" aria-label="How it starts">
      <div className="home-starts-head">
        <h2 className="home-section-title">How it starts</h2>
        {empty ? null : (
          <button type="button" className="home-starts-add" onClick={add}>
            <Icon name="plus" size={12} />
            Add entrance
          </button>
        )}
      </div>
      {entranceCount(view) > SEARCH_THRESHOLD ? (
        <SearchField
          value={query}
          onChange={(text) => {
            setSearch({ text, highlight });
          }}
          label="Search entrances"
          placeholder="Search"
          className="home-starts-search"
        />
      ) : null}
      {empty ? (
        <div className="home-starts-empty">
          <EmptyState
            title="No entrances yet"
            body="Add one from any card, or let an agent write them."
            action={
              <>
                <Button variant="primary" onClick={add}>
                  Add entrance
                </Button>
                <Button variant="secondary" onClick={copyAgentPrompt}>
                  Copy agent prompt
                </Button>
              </>
            }
          />
        </div>
      ) : rows.length > 0 ? (
        <div className="home-starts-list">
          {rows.map((group) => (
            <div key={group.id} className="home-start-app">
              {[group.app, ...group.entrances].map((row) => (
                <Row
                  key={row.id}
                  project={project}
                  row={row}
                  highlight={lit}
                  onChoose={(slot, anchor) => {
                    setChoosing({ slot, anchor });
                  }}
                />
              ))}
            </div>
          ))}
        </div>
      ) : (
        <p className="home-starts-note">No entrances match.</p>
      )}
      {adding ? (
        <AddEntrance
          project={project}
          anchor={adding}
          align="end"
          onClose={() => {
            setAdding(null);
          }}
        />
      ) : null}
      {choosing ? (
        <ChooseCode
          project={project}
          slot={choosing.slot}
          anchor={choosing.anchor}
          onClose={() => {
            setChoosing(null);
          }}
        />
      ) : null}
    </section>
  );
}
