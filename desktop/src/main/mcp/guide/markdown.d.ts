/** A guide topic, imported as its Markdown text (Vite's `?raw`, which electron-vite and Vitest both apply). */
declare module "*.md?raw" {
  const text: string;
  export default text;
}
