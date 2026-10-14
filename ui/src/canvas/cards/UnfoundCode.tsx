/**
 * Code an app or entrance starts at that isn't found, drawn as its own dashed card beside the start's
 * card (docs/engine.md#resolving-code): "Code not
 * found", the key as written in `unfold.toml`, and the fix the engine offers, never applied unasked. A
 * missing key that exactly one candidate fits says where it went, with Fix (one line rewritten); code
 * the scope leaves out offers `unfold.toml`, where `[scope]` is; code that isn't a start offers Choose
 * code…. The fix and the line it rewrites come from the project's entrances, matched by the start's key.
 */

import { useRef, useState, type ReactNode } from "react";
import { Popover, showToast } from "../../components";
import { CodeSearch } from "../../entrances";
import { toUnfoldError, useEngineQuery, type ProjectId } from "../../ipc";
import { call } from "../../ipc/commands";
import type { EntranceCode, EntrancesView, Key, StartTarget } from "../../ipc/generated";
import { openInEditor } from "../../lib/files";
import { rewriteCode, type CodeSlot } from "../../home/starts/model";

const CONFIG = "unfold.toml";

/** Code key `index` of the app or entrance whose key is `start`, as the project's entrances list it. */
function codeSlot(view: EntrancesView, start: Key, index: number): { readonly slot: CodeSlot; readonly code: EntranceCode | undefined } | null {
  for (const app of view.apps) {
    if (app.key === start) {
      return { slot: { app, entrance: null, index }, code: index === 0 ? app.code : undefined };
    }
    const entrance = app.entrances.find((item) => item.key === start);
    if (entrance) {
      return { slot: { app, entrance, index }, code: entrance.code[index] };
    }
  }
  return null;
}

/** Writes `code` in place of one code key; `entrances_changed` then redraws the card. */
function rewrite(project: ProjectId, slot: CodeSlot, code: Key): void {
  const edit = rewriteCode(slot, code);
  if (edit === null) {
    return;
  }
  call("update_project_config", { project, edits: [edit] }).catch((error: unknown) => {
    showToast({ title: "Couldn't change unfold.toml", body: toUnfoldError(error).message, tone: "danger" });
  });
}

interface UnfoundCodeProps {
  readonly project: ProjectId;
  /** The app's or entrance's key. */
  readonly start: Key;
  readonly code: StartTarget;
  /** Its place among the start's code. */
  readonly index: number;
}

export function UnfoundCode({ project, start, code, index }: UnfoundCodeProps) {
  const entrances = useEngineQuery("entrances", { project });
  const found = entrances.data ? codeSlot(entrances.data, start, index) : null;
  const field = useRef<HTMLInputElement>(null);
  // Choose code…'s search, beside the button that opened it.
  const [choosing, setChoosing] = useState<HTMLElement | null>(null);

  const action = (label: string, onClick: (button: HTMLElement) => void) => (
    <button
      type="button"
      className="start-unfound-fix nodrag"
      onClick={(event) => {
        event.stopPropagation();
        onClick(event.currentTarget);
      }}
    >
      {label}
    </button>
  );

  let what: string | null = null;
  let offer: ReactNode = null;
  const fix = found?.code?.fix;
  if (code.state === "missing" && found && fix) {
    what = fix.kind === "edited_route" ? "Route edited" : `Moved to ${fix.path}`;
    offer = action("Fix", () => {
      rewrite(project, found.slot, fix.code);
    });
  } else if (code.state === "not_analyzed") {
    what = "Not analyzed";
    offer = action("Fix", () => void openInEditor(project, CONFIG));
  } else if (code.state === "not_a_start") {
    what = "Not a start";
    offer = found ? action("Choose code…", setChoosing) : null;
  }

  return (
    <div className="start-unfound-row">
      <span className="start-unfound-link" aria-hidden="true" />
      <div className="start-unfound-card" data-key={code.key}>
        <div className="start-unfound-head">
          <span className="start-unfound-mark" aria-hidden="true">
            ?
          </span>
          <b>Code not found</b>
        </div>
        <div className="start-unfound-body">
          <code className="start-unfound-key">{code.key}</code>
          {what !== null ? <span className="start-unfound-what">{what}</span> : null}
          {offer}
        </div>
      </div>
      {choosing && found ? (
        <Popover
          open
          anchor={choosing}
          label="Choose code"
          align="start"
          width={400}
          initialFocus={field}
          onClose={() => {
            setChoosing(null);
          }}
        >
          <div className="start-unfound-choose">
            <CodeSearch
              ref={field}
              project={project}
              onPick={(hit) => {
                setChoosing(null);
                rewrite(project, found.slot, hit.key);
              }}
            />
          </div>
        </Popover>
      ) : null}
    </div>
  );
}
