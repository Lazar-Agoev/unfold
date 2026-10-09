let sink: (line: string) => void = console.log;

export function setLogSink(next: (line: string) => void): void {
  sink = next;
}

export const logger = {
  info(message: string, details?: unknown): void {
    const line = JSON.stringify({ level: "info", message, details });
    sink(line);
  },
  error(message: string, error?: unknown): void {
    const details = error instanceof Error ? error.message : error;
    const line = JSON.stringify({ level: "error", message, details });
    sink(line);
  },
};
