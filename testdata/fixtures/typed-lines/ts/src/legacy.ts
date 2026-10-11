// Written before the project had types: nothing here is annotated.
export function legacyTotal(cart) {
  return cart.items.length;
}

export function legacyReport(cart, user) {
  return `${user.email}: ${legacyTotal(cart)}`;
}
