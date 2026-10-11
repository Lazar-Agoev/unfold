/**
 * Analysis details: what Unfold would otherwise read from the command, set by hand. The target
 * (a script file, a module, a callable, or an ASGI app), the import paths in order (they replace
 * the ones the command gives), and the environment, which Unfold never copies from your shell.
 */

import { useId } from "react";
import { IconButton, TextButton } from "../components";
import { Icon } from "../icons";
import { TARGET_TYPES, emptyTarget, type EnvRow, type TargetDraft, type TargetType } from "./draft";

interface AnalysisDetailsProps {
  readonly open: boolean;
  readonly onOpen: (open: boolean) => void;
  readonly target: TargetDraft | null;
  readonly pythonPaths: readonly string[] | null;
  readonly env: readonly EnvRow[];
  /** The import paths the command gives, which "+ Add path" starts from. */
  readonly inferredPaths: readonly string[];
  readonly onTarget: (target: TargetDraft | null) => void;
  readonly onPythonPaths: (paths: readonly string[] | null) => void;
  readonly onEnv: (env: readonly EnvRow[]) => void;
  /** Sets the target and paths by hand, from what the preview read. */
  readonly onSetByHand: () => void;
  readonly onClear: () => void;
  readonly problems: { readonly target?: string | undefined; readonly pythonPaths?: string | undefined; readonly env?: string | undefined };
}

const FROM_COMMAND = "";

function TargetFields({ target, onTarget }: { readonly target: TargetDraft; readonly onTarget: (target: TargetDraft) => void }) {
  if (target.type === "file") {
    return (
      <input
        className="launch-input launch-mono launch-grow"
        aria-label="Target file"
        placeholder="src/main.py"
        value={target.path}
        spellCheck={false}
        autoComplete="off"
        onChange={(event) => {
          onTarget({ ...target, path: event.target.value });
        }}
      />
    );
  }
  return (
    <>
      <input
        className="launch-input launch-mono launch-grow"
        aria-label="Target module"
        placeholder="package.module"
        value={target.module}
        spellCheck={false}
        autoComplete="off"
        onChange={(event) => {
          onTarget({ ...target, module: event.target.value });
        }}
      />
      {target.type === "python_module" ? null : (
        <>
          <span className="launch-colon" aria-hidden="true">
            :
          </span>
          <input
            className="launch-input launch-mono launch-attribute"
            aria-label="Target attribute"
            placeholder="app"
            value={target.attribute}
            spellCheck={false}
            autoComplete="off"
            onChange={(event) => {
              onTarget({ ...target, attribute: event.target.value });
            }}
          />
        </>
      )}
    </>
  );
}

export function AnalysisDetails(props: AnalysisDetailsProps) {
  const { open, onOpen, target, pythonPaths, env, inferredPaths, onTarget, onPythonPaths, onEnv, onSetByHand, onClear, problems } = props;
  const bodyId = useId();
  const typeId = useId();
  const byHand = target !== null || pythonPaths !== null;
  const setPath = (position: number, value: string) => {
    onPythonPaths((pythonPaths ?? []).map((path, at) => (at === position ? value : path)));
  };
  const setEnv = (position: number, row: EnvRow) => {
    onEnv(env.map((current, at) => (at === position ? row : current)));
  };
  return (
    <section className={open ? "launch-details launch-details-open" : "launch-details"} aria-label="Analysis details">
      <div className="launch-details-head">
        <button
          type="button"
          className="launch-details-toggle"
          aria-expanded={open}
          aria-controls={open ? bodyId : undefined}
          onClick={() => {
            onOpen(!open);
          }}
        >
          <Icon name={open ? "chevron-down-small" : "chevron-right-small"} size={10} />
          <span className="launch-details-title">Analysis details</span>
          <span className="launch-details-state">{byHand ? "set by hand" : "read from the command"}</span>
        </button>
        {byHand ? <TextButton onClick={onClear}>Clear</TextButton> : <TextButton onClick={onSetByHand}>Set by hand</TextButton>}
      </div>
      {open ? (
        <div id={bodyId} className="launch-details-body">
          <div className="launch-details-group">
            <label className="launch-sublabel" htmlFor={typeId}>
              Target
            </label>
            <div className="launch-target">
              <div className="launch-select launch-select-native launch-target-type">
                <select
                  id={typeId}
                  value={target?.type ?? FROM_COMMAND}
                  onChange={(event) => {
                    const type = event.target.value;
                    onTarget(type === FROM_COMMAND ? null : { ...(target ?? emptyTarget()), type: type as TargetType });
                  }}
                >
                  <option value={FROM_COMMAND}>From the command</option>
                  {TARGET_TYPES.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
                <Icon name="chevron-down-small" size={10} />
              </div>
              {target ? <TargetFields target={target} onTarget={onTarget} /> : null}
            </div>
            {problems.target === undefined ? null : <p className="launch-error">{problems.target}</p>}
          </div>
          <div className="launch-details-group">
            <span className="launch-sublabel">Import paths, in order</span>
            {pythonPaths === null ? <p className="launch-details-note">From the command.</p> : null}
            {pythonPaths && pythonPaths.length > 0 ? (
              <ol className="launch-paths">
                {pythonPaths.map((path, position) => (
                  // Paths have no identity besides their position while they're typed.
                  <li key={position} className="launch-path">
                    <span className="launch-path-number">{position + 1}</span>
                    <input
                      className="launch-path-input launch-mono"
                      aria-label={`Import path ${position + 1}`}
                      value={path}
                      spellCheck={false}
                      autoComplete="off"
                      onChange={(event) => {
                        setPath(position, event.target.value);
                      }}
                    />
                    <IconButton
                      icon="close"
                      size={20}
                      label={`Remove import path ${position + 1}`}
                      onClick={() => {
                        const rest = pythonPaths.filter((_, at) => at !== position);
                        onPythonPaths(rest.length === 0 ? null : rest);
                      }}
                    />
                  </li>
                ))}
              </ol>
            ) : null}
            {problems.pythonPaths === undefined ? null : <p className="launch-error">{problems.pythonPaths}</p>}
            <TextButton
              className="launch-add"
              onClick={() => {
                onPythonPaths([...(pythonPaths ?? inferredPaths), ""]);
              }}
            >
              + Add path
            </TextButton>
          </div>
          <div className="launch-details-group">
            <div className="launch-env-head">
              <span className="launch-sublabel">Environment</span>
              {env.length === 0 ? <span className="launch-details-note launch-grow">None set. Unfold never reads your shell.</span> : <span className="launch-grow" />}
              <TextButton
                onClick={() => {
                  onEnv([...env, { key: "", value: "" }]);
                }}
              >
                + Add
              </TextButton>
            </div>
            {env.map((row, position) => (
              // Variables have no identity besides their position while they're typed.
              <div key={position} className="launch-env-row">
                <input
                  className="launch-input launch-mono launch-env-key"
                  aria-label={`Variable ${position + 1} name`}
                  placeholder="NAME"
                  value={row.key}
                  spellCheck={false}
                  autoComplete="off"
                  onChange={(event) => {
                    setEnv(position, { ...row, key: event.target.value });
                  }}
                />
                <span className="launch-colon" aria-hidden="true">
                  =
                </span>
                <input
                  className="launch-input launch-mono launch-grow"
                  aria-label={`Variable ${position + 1} value`}
                  value={row.value}
                  spellCheck={false}
                  autoComplete="off"
                  onChange={(event) => {
                    setEnv(position, { ...row, value: event.target.value });
                  }}
                />
                <IconButton
                  icon="close"
                  size={20}
                  label={`Remove variable ${position + 1}`}
                  onClick={() => {
                    onEnv(env.filter((_, at) => at !== position));
                  }}
                />
              </div>
            ))}
            {problems.env === undefined ? null : <p className="launch-error">{problems.env}</p>}
          </div>
        </div>
      ) : null}
    </section>
  );
}
