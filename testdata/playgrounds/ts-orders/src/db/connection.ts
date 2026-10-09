import { OrderRepository } from "./repository.ts";

const repository = new OrderRepository();

export function connect(): OrderRepository {
  return repository;
}
