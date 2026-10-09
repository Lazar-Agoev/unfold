// main.ts — imports from utils.ts and runs everything
import {
  add,
  classifyAge,
  describeRole,
  firstItem,
  sum,
  adults,
  formatUser,
  safeDivide,
} from "./utils.ts";
import type { User } from "./utils.ts";

console.log("--- basics ---");
console.log("add(2, 3) =", add(2, 3));
console.log("sum([1,2,3,4]) =", sum([1, 2, 3, 4]));

console.log("\n--- if / else ---");
for (const age of [-1, 5, 15, 30, 70]) {
  console.log(`age ${age} ->`, classifyAge(age));
}

console.log("\n--- switch ---");
console.log(describeRole("admin"));
console.log(describeRole("viewer"));

console.log("\n--- objects & arrays ---");
const users: User[] = [
  { id: 1, name: "Ada", age: 36, role: "admin", email: "ada@example.com" },
  { id: 2, name: "Linus", age: 17, role: "viewer" },
  { id: 3, name: "Grace", age: 45, role: "editor" },
];

users.forEach((u) => console.log(formatUser(u)));
console.log("adults:", adults(users).map((u) => u.name));

console.log("\n--- generics ---");
console.log(firstItem(users)?.name); // type is User | undefined
console.log(firstItem<number>([]));  // undefined

console.log("\n--- Result type ---");
for (const [a, b] of [[10, 2], [1, 0]]) {
  const result = safeDivide(a, b);
  if (result.ok) {
    console.log(`${a} / ${b} =`, result.value); // TS knows `value` exists here
  } else {
    console.log(`${a} / ${b} failed:`, result.error); // ...and `error` here
  }
}

// Try uncommenting these and run `npm run playground:check` from the repo root:
// add("2", 3);                         // string is not a number
// describeRole("superuser");           // not a valid Role
// const bad: User = { id: 1, name: "X" }; // missing age and role
