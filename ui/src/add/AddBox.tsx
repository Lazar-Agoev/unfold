/** The window's Add box, mounted once: it shows the box while Add is open. Each opening starts a fresh box. */

import type { JSX } from "react";
import { useAddOpen } from "../state/overlays";
import { AddLayer } from "./AddLayer";

export function AddBox(): JSX.Element | null {
  return useAddOpen() ? <AddLayer /> : null;
}
