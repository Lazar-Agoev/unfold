/** The command palette (⇧⌘P) and quick open (⌘P files, ⌘T symbols). */

export { CommandPalette } from "./CommandPalette";
export {
  closePalette,
  hasQuickOpenProvider,
  openPalette,
  registerQuickOpenProvider,
  switchPaletteMode,
  usePalette,
  useQuickOpenProvider,
  type PaletteMode,
  type QuickOpenItem,
  type QuickOpenMode,
  type QuickOpenProvider,
} from "./store";
