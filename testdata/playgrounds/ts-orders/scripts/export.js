import { readFile, writeFile } from "node:fs/promises";

async function exportOrders() {
  const [, , inputPath, outputPath] = process.argv;
  if (!inputPath) throw new Error("Usage: npm run legacy:export -- <input.json> [output.json]");
  const orders = JSON.parse(await readFile(inputPath, "utf8"));
  if (!Array.isArray(orders)) throw new Error("Orders file must contain an array");
  const rows = orders.map((order) => {
    if (!order || typeof order.id !== "string") throw new Error("Each order must have an id");
    const { id, customerId, status, createdAt } = order;
    return { id, customerId, status, createdAt };
  });
  const output = `${JSON.stringify(rows, null, 2)}\n`;
  if (outputPath) await writeFile(outputPath, output, { flag: "wx" });
  else process.stdout.write(output);
}

exportOrders().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
