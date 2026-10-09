export function startConsumer(queue: string, handler: (message: unknown) => void) {
  handler({ queue });
}
