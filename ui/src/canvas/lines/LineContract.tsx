/**
 * The selected line's contract, opened in place of its label (docs/proposals/typed-lines.md §7):
 * what the call sends (each argument as written beside the parameter it fills), what comes back and
 * where it lands, and what the callee throws (with "catch" when the call sits in a `try` that
 * catches). The caller's side is on the left, the callee's on the right. A written type opens its
 * definition under its row; which ones are open is the page's view state.
 */

import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { TypeDefinition } from "../../code/TypeDefinition";
import { TypeText } from "../../code/TypeText";
import type { CallContract, ContractReturn, ContractSend, ProjectId, TypeText as TypeTextModel } from "../../ipc/generated";
import { useEngineQuery } from "../../ipc/queries";
import { keyDisplayName, toggleOpenType, usePageView, type PageTarget, type SelectedLine } from "../../state/boards";
import "./lines.css";

/** What the caller's column says about the result: where it lands. */
export function bindingText(returns: ContractReturn): { readonly text: string; readonly dim: boolean } {
  switch (returns.binding) {
    case "variable":
      return returns.text !== undefined ? { text: returns.text, dim: false } : { text: "…", dim: true };
    case "return":
      return { text: "return", dim: false };
    case "argument":
      return returns.text !== undefined ? { text: `${returns.text}(…)`, dim: false } : { text: "…", dim: true };
    case "discard":
      return { text: "—", dim: true };
    case "other":
      return { text: "…", dim: true };
  }
}

/** What the caller's column says about one send: the argument as written, or why none fills it. */
export function sentText(send: ContractSend): { readonly text: string; readonly dim: boolean } {
  if (send.argument) {
    return send.argument.text !== undefined ? { text: send.argument.text, dim: false } : { text: "…", dim: true };
  }
  const parameter = send.parameter;
  return { text: parameter?.default === true ? "default" : parameter?.optional === true ? "optional" : "—", dim: true };
}

interface RowProps {
  readonly label: string;
  readonly caller: { readonly text: string; readonly dim: boolean };
  readonly children: ReactNode;
}

function Row({ label, caller, children }: RowProps) {
  return (
    <div className="line-contract-row">
      <span className="line-contract-label">{label}</span>
      <span className={`line-contract-caller${caller.dim ? " dim" : ""}`} title={caller.text}>
        {caller.text}
      </span>
      <span className="line-contract-callee">{children}</span>
    </div>
  );
}

interface TypedRowProps extends RowProps {
  readonly page: PageTarget;
  readonly project: ProjectId;
  /** The row's id in the page's open types. */
  readonly id: string;
  readonly type: TypeTextModel | undefined;
  readonly openTypes: Readonly<Record<string, number>>;
}

/** A row whose callee side ends in a written type, and the definition opened under it. */
function TypedRow({ page, project, id, type, openTypes, label, caller, children }: TypedRowProps) {
  const open = openTypes[id];
  const span = open !== undefined ? type?.spans[open] : undefined;
  const target = span?.style === "declared" ? span.target : undefined;
  const onToggle = (row: string, index: number) => {
    toggleOpenType(page, row, index);
  };
  return (
    <>
      <Row label={label} caller={caller}>
        {children}
        {type ? (
          <TypeText
            type={type}
            open={open ?? null}
            onToggle={(index) => {
              onToggle(id, index);
            }}
          />
        ) : null}
      </Row>
      {target !== undefined ? (
        <div className="line-contract-opened">
          <TypeDefinition project={project} typeKey={target} row={id} depth={1} openTypes={openTypes} onToggle={onToggle} />
        </div>
      ) : null}
    </>
  );
}

function Sends({ page, project, contract, openTypes }: { readonly page: PageTarget; readonly project: ProjectId; readonly contract: CallContract; readonly openTypes: Readonly<Record<string, number>> }) {
  if (contract.sends.length === 0) {
    return (
      <Row label="Sends" caller={{ text: "nothing", dim: true }}>
        {null}
      </Row>
    );
  }
  return (
    <>
      {contract.sends.map((send, index) => {
        const parameter = send.parameter;
        const name = parameter?.name;
        return (
          <TypedRow
            key={index}
            page={page}
            project={project}
            id={`send:${index}`}
            type={parameter?.type}
            openTypes={openTypes}
            label={index === 0 ? "Sends" : ""}
            caller={sentText(send)}
          >
            {send.unpaired ? <span className="line-contract-note">{send.unpaired.note}</span> : null}
            {name !== undefined ? <span className={parameter?.type ? "line-contract-param" : "line-contract-name"}>{parameter?.type ? `${name}: ` : name}</span> : null}
          </TypedRow>
        );
      })}
    </>
  );
}

export interface LineContractProps {
  readonly target: PageTarget;
  readonly line: SelectedLine;
  /** Where the label was (the curve's midpoint), in canvas units. */
  readonly x: number;
  readonly y: number;
}

export function LineContract({ target, line, x, y }: LineContractProps) {
  const { project, source, seq } = line;
  const query = useEngineQuery("call_contract", { project, source, seq });
  const openTypes = usePageView(target, (view) => view.openTypes);
  const contract = query.data;
  // Centered where the label was when it opens; a type opened inside grows it downward only.
  const element = useRef<HTMLElement>(null);
  const [lift, setLift] = useState<number | null>(null);
  const loaded = contract !== undefined;
  useLayoutEffect(() => {
    if (loaded && lift === null && element.current) {
      setLift(element.current.offsetHeight / 2);
    }
  }, [loaded, lift]);
  useLayoutEffect(() => {
    element.current?.focus({ preventScroll: true });
  }, []);

  const name = contract ? keyDisplayName(contract.target) : "the callee";
  let body: ReactNode;
  if (!contract) {
    body = <p className="line-contract-message">{query.isError ? query.error.message : "Reading the call…"}</p>;
  } else {
    const returns = contract.returns;
    body = (
      <>
        <div className="line-contract-section">
          <Sends page={target} project={project} contract={contract} openTypes={openTypes} />
        </div>
        <div className="line-contract-section">
          <TypedRow page={target} project={project} id="returns" type={returns.type} openTypes={openTypes} label="Returns" caller={bindingText(returns)}>
            {returns.type ? null : <span className="line-contract-note">?</span>}
          </TypedRow>
          {contract.throws.map((thrown, index) => (
            <TypedRow
              key={index}
              page={target}
              project={project}
              id={`throw:${index}`}
              type={thrown.type}
              openTypes={openTypes}
              label={index === 0 ? "Throws" : ""}
              caller={contract.caught ? { text: "catch", dim: true } : { text: "", dim: true }}
            >
              {null}
            </TypedRow>
          ))}
        </div>
      </>
    );
  }
  const transform = lift === null ? `translate(-50%, -50%) translate(${x}px, ${y}px)` : `translate(-50%, 0) translate(${x}px, ${y - lift}px)`;
  return (
    <section
      ref={element}
      className="line-contract nodrag nopan nowheel"
      style={{ transform }}
      aria-label={`Call to ${name}`}
      tabIndex={-1}
      onClick={(event) => {
        event.stopPropagation();
      }}
    >
      {body}
    </section>
  );
}
