/**
 * Prints what the journeys measured (the `ms` annotations, such as save → updated card) and where
 * a failed test's sandbox was kept, after each test.
 */

import type { Reporter, TestCase, TestResult } from "@playwright/test/reporter";

export default class MeasurementsReporter implements Reporter {
  onTestEnd(test: TestCase, _result: TestResult): void {
    for (const annotation of test.annotations) {
      if (annotation.description !== undefined && (annotation.type === "sandbox" || /\d\s*(ms|MB|fps)\b/.test(annotation.description))) {
        process.stdout.write(`      ${annotation.type}: ${annotation.description}\n`);
      }
    }
  }

  printsToStdio(): boolean {
    return false;
  }
}
