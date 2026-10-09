import { service } from "./a.js";
import * as all from "./a.js";
service.start();
all.service.stop();
delete (service as any).arrow;
