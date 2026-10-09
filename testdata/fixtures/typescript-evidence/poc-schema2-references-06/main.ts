import external from "remote";
import express from "local";
function inspect(...args: unknown[]) {}
const app = express();
function owner(external: () => void) { inspect(express, app, external); }
