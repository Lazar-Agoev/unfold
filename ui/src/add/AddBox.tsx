/** The window's Add box, mounted once: it shows the box while Add is open on a board. Each opening starts a fresh box. */

import type { JSX } from "react";
import { useAddOpen, useAddPage } from "../state/overlays";
import { AddLayer } from "./AddLayer";

export function AddBox(): JSX.Element | null {
  const open = useAddOpen();
  const page = useAddPage();
  return open && page ? <AddLayer page={page} /> : null;
}
