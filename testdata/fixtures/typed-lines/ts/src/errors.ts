export class PaymentError extends Error {
  constructor(message: string) {
    super(message);
  }
}

export class CardDeclinedError extends PaymentError {
  constructor(public readonly intent: string) {
    super(`card declined for ${intent}`);
  }
}

export class OutOfStockError extends Error {}
