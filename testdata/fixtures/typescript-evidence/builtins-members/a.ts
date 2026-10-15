export function elements(s: string, text: string, index: number): void {
  s.toLowerCase();
  s[0].toLowerCase();
  text[index].toLowerCase();
}

export function properties(s: string, e: Error, f: () => void): void {
  s.length.toFixed();
  e.message.trim();
  Math.PI.toFixed(2);
  Function.prototype.toString.call(f);
  s.trim.call(s);
  console.log.bind(console);
  console.log("host");
  Error.captureStackTrace(e);
  (Error as any).captureStackTrace(e);
}

export function namespaces(locale: string): void {
  Intl.Collator(locale);
  new Intl.Collator(locale).compare("a", "b");
  Intl.Collator.supportedLocalesOf(locale);
  Intl.getCanonicalLocales(locale);
  Reflect.ownKeys({});
}

export function globalObject(): void {
  globalThis.JSON.parse("{}");
  new globalThis.Map<string, number>().get("a");
  globalThis.console.log("host");
  new ReadableStream<Uint8Array>().getReader();
  new URL("https://example.com").toString();
}
