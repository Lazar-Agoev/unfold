import express from 'express'; import { handler as renamed } from './barrel.js'; import external from 'middleware';
function first() {} function wrapper(fn: unknown) { return fn; } let mutable = () => {}; class Handler {}
const app = express();
app.get('/handlers', [first, [(renamed)!, , ...rest]], () => {}, function inline() {}, wrapper(first), { first }, mutable, Handler, external, missing, ...tail);
function register(parameter: () => void) { app.post('/parameter', parameter); }
app.get('/empty', [], [[]]); app.get('/no-handlers');
