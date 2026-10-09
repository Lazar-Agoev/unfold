import { Worker } from "bullmq"; import { URL as FileURL } from "node:url";
new Worker("file", "./processor.js"); new Worker("template", `./template.js`);
new Worker("escaped", "./line\nprocessor.js");
new Worker("url", new FileURL( /* portable */ "./processor.js", import.meta.url )); new Worker("wrapped", (new FileURL( /* portable */ "./processor.js", import.meta.url )));
new Worker("long", "./pppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppp.js");
class URL { constructor(...args: unknown[]) {} } new Worker("local", new URL("./processor.js"));
const stored = new FileURL("./processor.js", import.meta.url); new Worker("stored", stored);
