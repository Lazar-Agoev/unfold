// utils.ts — types, functions, and control flow to play with

// A union type: a value can only be one of these exact strings
export type Role = "admin" | "editor" | "viewer";

// An interface describes the shape of an object
export interface User {
  id: number;
  name: string;
  age: number;
  role: Role;
  email?: string; // optional property
}

// Basic typed function: params and return type are annotated
export function add(a: number, b: number): number {
  return a + b;
}

// if / else if / else
export function classifyAge(age: number): string {
  if (age < 0) {
    return "invalid";
  } else if (age < 13) {
    return "child";
  } else if (age < 20) {
    return "teenager";
  } else if (age < 65) {
    return "adult";
  } else {
    return "senior";
  }
}

// switch on a union type — try adding a new Role above and see the compiler
// complain here (if you run `npx tsc --noEmit`) because of the `never` check
export function describeRole(role: Role): string {
  switch (role) {
    case "admin":
      return "Can do everything";
    case "editor":
      return "Can edit content";
    case "viewer":
      return "Read-only access";
    default: {
      const _exhaustive: never = role;
      return _exhaustive;
    }
  }
}

// Generics: works for any type T
export function firstItem<T>(items: T[]): T | undefined {
  return items[0];
}

// Loops + array methods
export function sum(numbers: number[]): number {
  let total = 0;
  for (const n of numbers) {
    total += n;
  }
  return total;
}

export function adults(users: User[]): User[] {
  return users.filter((u) => u.age >= 18);
}

// Optional property handling (narrowing): TS forces you to check for undefined
export function formatUser(user: User): string {
  const email = user.email !== undefined ? user.email : "no email";
  return `#${user.id} ${user.name} (${user.age}, ${user.role}) <${email}>`;
}

// A function that can fail: returns a "result" object instead of throwing
export type Result<T> = { ok: true; value: T } | { ok: false; error: string };

export function safeDivide(a: number, b: number): Result<number> {
  if (b === 0) {
    return { ok: false, error: "Cannot divide by zero" };
  }
  return { ok: true, value: a / b };
}
