import { save } from "./db.ts";
import { Repo } from "./repo.ts";

export class OrderService {
  constructor(private repo: Repo) {}

  place(prices: number[]) {
    const total = this.sum(prices);
    prices.forEach((price) => save(price));
    return this.repo.insert(total);
  }

  sum(prices: number[]) {
    return prices.reduce((a, b) => a + b, 0);
  }
}
