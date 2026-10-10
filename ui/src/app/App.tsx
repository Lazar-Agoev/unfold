/**
 * The window: the tab strip, then the sidebar and the content area, then the window-level
 * layers (Add, dialogs, toasts). It wires the window up, registers the frame's actions, and adds folders
 * dropped anywhere on it as projects.
 */

import { AddBox } from "../add";
import { Toaster } from "../components";
import { DialogHost } from "../dialogs";
import { CompiledToasts } from "../home";
import { useProjectActions } from "../menus";
import { useGlobalActions } from "./actions";
import { Content } from "./Content";
import { onWindowDragOver, onWindowDrop } from "./drop";
import { Sidebar } from "./Sidebar";
import { TabStrip } from "./TabStrip";
import { useWindowWiring } from "./wiring";
import "./app.css";

export function App() {
  useWindowWiring();
  useGlobalActions();
  useProjectActions();
  return (
    <div className="app" onDragOver={onWindowDragOver} onDrop={onWindowDrop}>
      <TabStrip />
      <div className="app-body">
        <Sidebar />
        <Content />
      </div>
      <AddBox />
      <DialogHost />
      <Toaster />
      <CompiledToasts />
    </div>
  );
}
