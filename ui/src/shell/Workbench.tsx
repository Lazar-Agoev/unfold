/**
 * The IDE-shaped workbench (architecture §11.1): activity bar, sidebar, editor, inspector, panel.
 * The sidebar, inspector, and panel resize from their inner edges; sizes live in the layout store.
 */

import { ResizablePane } from "../components";
import { PANE_BOUNDS, useLayout, type Pane } from "../state/layout";
import { ActivityBar } from "./ActivityBar";
import { BottomPanel } from "../panel/BottomPanel";
import { EditorArea } from "../editor/EditorArea";
import { Inspector } from "../inspector/Inspector";
import { BoardChrome } from "../boards/BoardChrome";
import { useSearchQuickOpen } from "../views/search/quickOpen";
import { Sidebar } from "./Sidebar";
import "./Workbench.css";

function usePaneSize(pane: Pane) {
  const size = useLayout((state) => state.sizes[pane]);
  const setPaneSize = useLayout((state) => state.setPaneSize);
  return {
    size,
    bounds: PANE_BOUNDS[pane],
    onResize: (next: number) => {
      setPaneSize(pane, next);
    },
  };
}

export function Workbench() {
  useSearchQuickOpen();
  const sidebarVisible = useLayout((state) => state.sidebarVisible);
  const inspectorOpen = useLayout((state) => state.inspectorOpen);
  const panelOpen = useLayout((state) => state.panelOpen);
  const sidebar = usePaneSize("sidebar");
  const inspector = usePaneSize("inspector");
  const panel = usePaneSize("panel");

  return (
    <div className="workbench">
      <BoardChrome />
      <ActivityBar />
      {sidebarVisible ? (
        <ResizablePane id="workbench-sidebar" className="workbench-sidebar" edge="right" label="Resize sidebar" {...sidebar}>
          <Sidebar />
        </ResizablePane>
      ) : null}
      <EditorArea />
      {inspectorOpen ? (
        <ResizablePane id="workbench-inspector" className="workbench-inspector" edge="left" label="Resize inspector" {...inspector}>
          <Inspector />
        </ResizablePane>
      ) : null}
      {panelOpen ? (
        <ResizablePane id="workbench-panel" className="workbench-panel" edge="top" label="Resize panel" {...panel}>
          <BottomPanel />
        </ResizablePane>
      ) : null}
    </div>
  );
}
