export interface User {
  id: string;
  email: string;
  address?: Address;
}

import type { Address } from "./cart.js";

export class Session {
  expiresAt: Date = new Date();

  constructor(
    public user: User,
    private readonly token: string,
  ) {}

  isExpired(now: Date): boolean {
    return now > this.expiresAt;
  }
}
