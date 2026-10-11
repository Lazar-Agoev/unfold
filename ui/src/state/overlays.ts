/**
 * What the window shows over its content: the Add box, the peek sheet, and one dialog
 * at a time. Each is a request any screen or action can make; the hosts (`AddBox`, `PeekSheet`,
 * `DialogHost`) render them. None of this is saved with the window.
 */

import { create } from "zustand";
import type { ApplicationSpec, ImportedLaunch, ProjectId, TextMatch } from "../ipc/generated";
import { boardKey } from "./boards/keys";
import { useBoards } from "./boards/store";
import type { BoardRef, PageTarget } from "./boards/types";
import { useActiveBoard } from "./tabs";
import { useWindowStore } from "./window";

export type AddKind = "entrances" | "functions" | "files" | "text";

export interface AddRequest {
  /** `commands` opens Add in its `>` mode. */
  readonly mode: "find" | "commands";
  readonly kind?: AddKind;
  readonly query?: string;
}

/** Where Add puts what it adds: the active board's active page, or Home with the selected project. */
export type AddTarget = { readonly kind: "page"; readonly target: PageTarget } | { readonly kind: "home"; readonly project: ProjectId | null };

/** A file shown in the peek sheet, optionally at a line, with text matches to step through. */
export interface PeekTarget {
  readonly project: ProjectId;
  readonly path: string;
  readonly line?: number;
  readonly matches?: readonly TextMatch[];
  /** The index of the current match in `matches`. */
  readonly current?: number;
}

export type ProjectSettingsSection = "general" | "scope" | "detectors" | "python" | "evidence";

/** A definition the launch editor starts a handler from ("Add as entrance…" on a card): its header's facts. */
export interface DefinitionRef {
  readonly path: string;
  /** The block kind, such as `function` or `method`. */
  readonly kind: string;
  readonly name: string;
  readonly qualifiedName: string;
  readonly line: number | null;
}

/** What a new launch starts with: from a suggestion, a deploy file, or "Add launch". */
export interface LaunchPrefill {
  readonly id?: string;
  readonly command?: string;
  readonly cwd?: string;
  readonly label?: string;
  /** An existing application, or a new one the save also declares. */
  readonly application?: ApplicationSpec;
  readonly schedule?: string;
  readonly description?: string;
  readonly env?: Readonly<Record<string, string>>;
  /** Where the command came from, such as `data-pipelines/Dockerfile.workers, CMD`. */
  readonly copiedFrom?: string;
}

export type DialogRequest =
  | { readonly kind: "launchEditor"; readonly project: ProjectId; readonly mode: "add"; readonly intent?: "process" | "handler"; readonly prefill?: LaunchPrefill; readonly handler?: DefinitionRef }
  | { readonly kind: "launchEditor"; readonly project: ProjectId; readonly mode: "edit"; readonly id: string }
  | { readonly kind: "launchEditor"; readonly project: ProjectId; readonly mode: "override"; readonly key: string }
  | { readonly kind: "launchEditor"; readonly project: ProjectId; readonly mode: "review"; readonly launches: readonly ImportedLaunch[] }
  | { readonly kind: "preferences" }
  | { readonly kind: "projectSettings"; readonly project: ProjectId; readonly section?: ProjectSettingsSection }
  | { readonly kind: "removeEntrance"; readonly project: ProjectId; readonly id: string; readonly name: string }
  | { readonly kind: "publish"; readonly board: BoardRef }
  | { readonly kind: "discard"; readonly board: BoardRef }
  | { readonly kind: "deleteBoard"; readonly board: BoardRef }
  | { readonly kind: "newFolder"; readonly project: ProjectId; readonly parent: string | null }
  | { readonly kind: "renameFolder"; readonly project: ProjectId; readonly path: string }
  | { readonly kind: "groupFromLines"; readonly target: PageTarget; readonly cardId: string };

interface OverlaysState {
  readonly add: AddRequest | null;
  readonly peek: PeekTarget | null;
  readonly dialog: DialogRequest | null;
}

const useOverlays = create<OverlaysState>()(() => ({ add: null, peek: null, dialog: null }));

/** Opens Add (find mode by default), or changes the request of the open one. */
export function openAdd(request: AddRequest = { mode: "find" }): void {
  useOverlays.setState({ add: request });
}

export function closeAdd(): void {
  useOverlays.setState({ add: null });
}

export function useAddRequest(): AddRequest | null {
  return useOverlays((state) => state.add);
}

/** Where Add adds, kept current: the active board tab's active page, else Home and its selected project. */
export function useAddTarget(): AddTarget {
  const board = useActiveBoard();
  const page = useBoards((state) => (board ? state.boards[boardKey(board)]?.activePage : undefined));
  const project = useWindowStore((state) => state.selectedProject);
  if (board && page !== undefined) {
    return { kind: "page", target: { board, page } };
  }
  return { kind: "home", project };
}

/** Shows a file in the peek sheet (one per window, kept across tab switches). */
export function openPeek(target: PeekTarget): void {
  useOverlays.setState({ peek: target });
}

export function closePeek(): void {
  useOverlays.setState({ peek: null });
}

export function usePeekTarget(): PeekTarget | null {
  return useOverlays((state) => state.peek);
}

/** Opens a dialog, replacing any open one. */
export function openDialog(request: DialogRequest): void {
  useOverlays.setState({ dialog: request });
}

export function closeDialog(): void {
  useOverlays.setState({ dialog: null });
}

export function useDialog(): DialogRequest | null {
  return useOverlays((state) => state.dialog);
}
