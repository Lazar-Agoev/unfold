/**
 * The window: the sidebar and the content area side by side, then the window-level layers (Add,
 * dialogs, toasts). It wires the window up.
 */

import { AddBox } from "../add";
import { Toaster } from "../components";
import { DialogHost } from "../dialogs";
import { CompiledToasts } from "../home";
import { Content } from "./Content";
import { Sidebar } from "./Sidebar";
import { useWindowWiring } from "./wiring";
import "./app.css";

export function App() {
  useWindowWiring();
  return (
    <div className="app">
      <Sidebar />
      <Content />
      <AddBox />
      <DialogHost />
      <Toaster />
      <CompiledToasts />
    </div>
  );
}
