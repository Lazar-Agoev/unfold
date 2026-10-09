// Shared ESLint flat config for the JavaScript workspace (desktop/, ui/).
import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: ["**/node_modules/**", "**/dist/**", "**/out/**", "**/coverage/**", "crates/**", "target/**"],
  },
  js.configs.recommended,
  ...tseslint.configs.strictTypeChecked,
  ...tseslint.configs.stylisticTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      "@typescript-eslint/consistent-type-imports": "error",
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_", varsIgnorePattern: "^_" }],
      "@typescript-eslint/restrict-template-expressions": ["error", { allowNumber: true }],
      eqeqeq: ["error", "always"],
      "no-console": ["error", { allow: ["warn", "error"] }],
    },
  },
  {
    files: ["desktop/**/*.ts"],
    languageOptions: { globals: { ...globals.node } },
  },
  {
    files: ["ui/src/**/*.{ts,tsx}"],
    languageOptions: { globals: { ...globals.browser } },
    plugins: { "react-hooks": reactHooks },
    rules: {
      ...reactHooks.configs.recommended.rules,
      // The UI talks to the app only through window.unfold; it never uses browser storage.
      "no-restricted-globals": [
        "error",
        { name: "localStorage", message: "The UI never uses browser storage; persist through Rust." },
        { name: "sessionStorage", message: "The UI never uses browser storage; persist through Rust." },
        { name: "indexedDB", message: "The UI never uses browser storage; persist through Rust." },
      ],
    },
  },
  {
    // Build and lint configuration files run in Node and sit outside the type-checked projects.
    files: ["**/*.config.{ts,mjs,js}"],
    extends: [tseslint.configs.disableTypeChecked],
    languageOptions: { globals: { ...globals.node } },
  },
);
