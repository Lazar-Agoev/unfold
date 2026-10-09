/**
 * Typed engine commands and events: the generated types (`./generated`) laid over the transport
 * client (`./unfold`). `call("list_boards", { project })` resolves with a `BoardList`.
 *
 * The casts here are the trust boundary: Rust produced the JSON from the same types the generated
 * module was made from, so the shapes are checked once, at build time, on both sides.
 */

import type {
  CommandArgs,
  CommandName,
  CommandResult,
  EventType,
  Events,
  StreamCommandName,
  StreamMessage,
} from "./generated";
import { invoke, onEvent, stream } from "./unfold";

/** Runs one engine command. Rejects with an `UnfoldError`. */
export async function call<C extends CommandName>(command: C, args: CommandArgs<C>): Promise<CommandResult<C>> {
  return (await invoke(command, args)) as CommandResult<C>;
}

/** A running streamed command, typed. */
export interface TypedStream<C extends StreamCommandName> {
  readonly result: Promise<CommandResult<C>>;
  readonly cancel: () => void;
}

/** Runs a streamed engine command (`compile`, `search_text`); `onMessage` gets each message. */
export function callStream<C extends StreamCommandName>(
  command: C,
  args: CommandArgs<C>,
  onMessage: (message: StreamMessage<C>) => void,
): TypedStream<C> {
  const handle = stream(command, args, (message) => {
    onMessage(message as StreamMessage<C>);
  });
  return {
    result: handle.result as Promise<CommandResult<C>>,
    cancel: () => {
      handle.cancel();
    },
  };
}

/** Subscribes to one engine event type. Returns the unsubscribe function. */
export function onEngineEvent<E extends EventType>(type: E, handler: (event: Events[E]) => void): () => void {
  return onEvent(type, (event) => {
    handler(event as unknown as Events[E]);
  });
}

