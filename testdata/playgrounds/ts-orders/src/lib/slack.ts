import { logger } from "./logger.ts";

export const slack = {
  post(channel: string, text: string): void {
    logger.info("Team notification", { channel, text });
  },
};
