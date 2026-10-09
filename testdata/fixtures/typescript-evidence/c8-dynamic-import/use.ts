export async function main() {
  const { printReport } = await import("./report.js");
  printReport();
  const { other: renamed, value = 0, ...rest } = await import("./report.js");
  renamed();
  let { printReport: mutable } = await import("./report.js");
  mutable();
  const promised = import("./report.js");
  const { printReport: notAwaited } = promised as any;
  notAwaited();
  const { printReport: fromExpression } = await import(`./report.js`);
  fromExpression();
  const { express } = await import("express");
  express();
}
