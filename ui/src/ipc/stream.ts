/**
 * Streamed commands where only the latest request matters: each run cancels the one
 * before it, and messages or a result arriving late from a superseded run are dropped. Add's text
 * search runs one per keystroke this way.
 */

import { callStream, type TypedStream } from "./commands";
import type { CommandArgs, CommandResult, StreamCommandName, StreamMessage } from "./generated";
import { UnfoldError } from "./unfold";

export interface LatestStream<C extends StreamCommandName> {
  /**
   * Cancels the running request and starts this one. Resolves with its result; rejects with code
   * `cancelled` when a later run or `cancel` superseded it.
   */
  run(args: CommandArgs<C>, onMessage: (message: StreamMessage<C>) => void): Promise<CommandResult<C>>;
  /** Cancels the running request; its late messages are dropped. */
  cancel(): void;
}

function superseded(): UnfoldError {
  return new UnfoldError({ code: "cancelled", message: "A newer request replaced this one." });
}

export function createLatestStream<C extends StreamCommandName>(command: C): LatestStream<C> {
  // Each run belongs to a generation; anything from an older generation is dropped.
  let generation = 0;
  let current: TypedStream<C> | null = null;

  const stop = () => {
    generation += 1;
    current?.cancel();
    current = null;
  };

  return {
    run: async (args, onMessage) => {
      stop();
      const mine = generation;
      const handle = callStream(command, args, (message) => {
        if (mine === generation) {
          onMessage(message);
        }
      });
      current = handle;
      try {
        const result = await handle.result;
        if (mine !== generation) {
          throw superseded();
        }
        return result;
      } catch (error) {
        throw mine === generation ? error : superseded();
      } finally {
        if (current === handle) {
          current = null;
        }
      }
    },
    cancel: stop,
  };
}
