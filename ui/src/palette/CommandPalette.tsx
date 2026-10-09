/**
 * The command palette overlay. In `commands` mode it lists every enabled action with its shortcut,
 * fuzzy-filtered by cmdk. In quick-open modes it shows a provider's results for the typed query
 * (debounced, cancellable); typing `>` first switches to commands.
 */

import { Command } from "cmdk";
import { useEffect, useRef, useState } from "react";
import { Kbd } from "../components/Kbd";
import { Portal, useModal } from "../components/internal/layers";
import { Spinner } from "../components/Progress";
import { actions as windowActions, useActions, type ActionRegistry } from "../keyboard";
import "./CommandPalette.css";
import {
  closePalette,
  switchPaletteMode,
  usePalette,
  type PaletteMode,
  type QuickOpenItem,
  type QuickOpenProvider,
} from "./store";

const QUICK_OPEN_DEBOUNCE_MS = 60;

interface Results {
  readonly query: string;
  readonly items: readonly QuickOpenItem[];
  readonly error: string | null;
}

/** Groups results under their headings, keeping the provider's order. */
function groupResults(items: readonly QuickOpenItem[]): { heading: string | null; items: QuickOpenItem[] }[] {
  const groups: { heading: string | null; items: QuickOpenItem[] }[] = [];
  for (const item of items) {
    const heading = item.group ?? null;
    let group = groups.find((entry) => entry.heading === heading);
    if (!group) {
      group = { heading, items: [] };
      groups.push(group);
    }
    group.items.push(item);
  }
  return groups;
}

function CommandItems({ registry, onRun }: { registry: ActionRegistry; onRun: (run: () => void) => void }) {
  const all = useActions(registry);
  // Evaluated once per opening: the palette is modal, so nothing changes underneath it.
  const [visible] = useState(() => all.filter((action) => action.showInPalette !== false && registry.isEnabled(action.id)));
  return (
    <>
      <Command.Empty className="palette-empty">No matching commands</Command.Empty>
      {visible.map((action) => (
        <Command.Item
          key={action.id}
          value={action.id}
          keywords={[action.category, action.title, ...(action.keywords ?? [])]}
          className="palette-item"
          onSelect={() => {
            onRun(() => {
              registry.run(action.id);
            });
          }}
        >
          <span className="palette-item-label">
            <span className="palette-item-category">{action.category}: </span>
            {action.title}
          </span>
          {action.shortcuts[0] ? <Kbd shortcut={action.shortcuts[0]} className="palette-item-kbd" /> : null}
        </Command.Item>
      ))}
    </>
  );
}

function QuickOpenItems({
  provider,
  query,
  onRun,
}: {
  provider: QuickOpenProvider;
  query: string;
  onRun: (run: () => void) => void;
}) {
  const [results, setResults] = useState<Results | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    const timer = window.setTimeout(
      () => {
        provider.search(query, controller.signal).then(
          (items) => {
            if (!controller.signal.aborted) {
              setResults({ query, items, error: null });
            }
          },
          (error: unknown) => {
            if (!controller.signal.aborted) {
              setResults({ query, items: [], error: error instanceof Error ? error.message : String(error) });
            }
          },
        );
      },
      query === "" ? 0 : QUICK_OPEN_DEBOUNCE_MS,
    );
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [provider, query]);

  const loading = results?.query !== query;
  // Keep showing the previous results while the next ones load, so the list doesn't flicker.
  const shown = results?.items ?? [];
  return (
    <>
      {loading ? (
        <Command.Loading className="palette-loading">
          <Spinner size={13} label="Searching" />
        </Command.Loading>
      ) : null}
      {results?.error ? (
        <div className="palette-empty" role="alert">
          {results.error}
        </div>
      ) : null}
      {results && !loading && results.error === null && shown.length === 0 ? (
        <div className="palette-empty">{provider.emptyMessage ?? "No results"}</div>
      ) : null}
      {groupResults(shown).map((group) => {
        const items = group.items.map((item) => (
          <Command.Item
            key={item.id}
            value={item.id}
            className="palette-item"
            onSelect={() => {
              onRun(item.run);
            }}
          >
            {item.icon === undefined ? null : (
              <span className="palette-item-icon" aria-hidden="true">
                {item.icon}
              </span>
            )}
            <span className="palette-item-label">{item.label}</span>
            {item.detail === undefined ? null : <span className="palette-item-detail">{item.detail}</span>}
          </Command.Item>
        ));
        return group.heading === null ? (
          items
        ) : (
          <Command.Group key={group.heading} heading={group.heading} className="palette-group">
            {items}
          </Command.Group>
        );
      })}
    </>
  );
}

const PLACEHOLDERS: Readonly<Record<PaletteMode, string>> = {
  commands: "Type a command",
  files: "Go to file",
  symbols: "Go to symbol",
};

const LABELS: Readonly<Record<PaletteMode, string>> = {
  commands: "Command palette",
  files: "Quick open: files",
  symbols: "Quick open: symbols",
};

function PaletteDialog({ mode, initialQuery, registry }: { mode: PaletteMode; initialQuery: string; registry: ActionRegistry }) {
  const [query, setQuery] = useState(initialQuery);
  const provider = usePalette((state) => (mode === "commands" ? undefined : state.providers[mode]));
  const afterClose = useRef<(() => void) | null>(null);
  useModal(true);
  // Declared after `useModal`, so on unmount it runs after focus has returned to the window: an
  // action that moves focus (Focus search) isn't undone by the palette closing.
  useEffect(
    () => () => {
      const run = afterClose.current;
      afterClose.current = null;
      run?.();
    },
    [],
  );

  const onRun = (run: () => void) => {
    afterClose.current = run;
    closePalette();
  };

  return (
    <Portal>
      <div
        className="palette-backdrop"
        onPointerDown={(event) => {
          if (event.target === event.currentTarget) {
            closePalette();
          }
        }}
      >
        <Command
          label={LABELS[mode]}
          className="palette"
          shouldFilter={mode === "commands"}
          loop
          vimBindings={false}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              closePalette();
            }
          }}
        >
          <Command.Input
            autoFocus
            className="palette-input"
            value={query}
            placeholder={mode === "commands" ? PLACEHOLDERS.commands : (provider?.placeholder ?? PLACEHOLDERS[mode])}
            onValueChange={(next) => {
              if (mode !== "commands" && next.startsWith(">")) {
                switchPaletteMode("commands", next.slice(1));
                return;
              }
              setQuery(next);
            }}
          />
          <Command.List className="palette-list">
            {mode === "commands" ? (
              <CommandItems registry={registry} onRun={onRun} />
            ) : provider ? (
              <QuickOpenItems provider={provider} query={query} onRun={onRun} />
            ) : (
              <div className="palette-empty">Quick open isn't available in this window yet.</div>
            )}
          </Command.List>
        </Command>
      </div>
    </Portal>
  );
}

/** Renders the palette while it is open. Mount once, in the shell. */
export function CommandPalette({ registry = windowActions }: { registry?: ActionRegistry }) {
  const { open, mode, initialQuery, session } = usePalette();
  if (!open) {
    return null;
  }
  return <PaletteDialog key={session} mode={mode} initialQuery={initialQuery} registry={registry} />;
}

