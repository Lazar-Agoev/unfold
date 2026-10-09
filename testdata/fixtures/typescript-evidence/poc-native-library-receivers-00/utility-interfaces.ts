export {};
interface Host { run(): void; }
function use(a: Pick<Host, "run">, b: Readonly<Host>, c: Required<Host>) { a.run(); b.run(); c.run(); }