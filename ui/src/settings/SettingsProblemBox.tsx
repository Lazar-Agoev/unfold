/** Rust's refusal of a settings change, with where in `unfold.toml` it found the problem. */

import { useEffect, useRef } from "react";
import { problemText, type SettingsProblemText } from "./edits";
import "./settings.css";

export function SettingsProblemBox({ problem }: { readonly problem: SettingsProblemText }) {
  const box = useRef<HTMLDivElement>(null);
  // The dialog may be scrolled away from where the refusal appears.
  useEffect(() => {
    box.current?.scrollIntoView({ block: "nearest" });
  }, [problem]);
  return (
    <div ref={box} className="settings-problem" role="alert">
      <strong>{problem.code === "settings_conflict" ? "Can't make this change" : "Unfold can't save this"}</strong>
      <span>{problemText(problem)}</span>
      {problem.line === null ? null : <span className="settings-problem-where">Position in unfold.toml as it would be written.</span>}
    </div>
  );
}

