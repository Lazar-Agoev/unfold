import type { Application as App, Router } from "express";
import type * as ExpressTypes from "express";
class Local {}
function use(app: App, router: ExpressTypes.Router, union: App | Router, array: App[], parenthesized: (App), query: typeof use, local: Local, missing: Unknown) {}
class Holder { app!: App; router!: ExpressTypes.Router; get returned(): App { throw 0; } }
type Alias = App; let alias: Alias;
function shadow<App>(parameter: App) {}