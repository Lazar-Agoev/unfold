import express, { Router, type Router as RouterType } from 'express';
import otherExpress from 'other-express'; import * as expressNamespace from 'express';
function handler() {} const app = express(); const unrelated = otherExpress(); unrelated.get('/wrong-package', handler);
const local = { get(...args: any[]) {}, post(...args: any[]) {}, use(...args: any[]) {} }; local.get('/local', handler);
function shadow(express: any) { const app = express(); app.post('/shadow', handler); }
let mutable = express(); mutable.get('/mutable', handler);
const alias = app; alias.get('/alias', handler);
const ns = expressNamespace.default(); ns.get('/namespace', handler);
app['get']('/computed', handler); app.route('/chain').get(handler); app.param('id', handler); app.use(handler);
function unknownFactory() { return {}; }
const router: RouterType = unknownFactory(); router.get('/annotation', handler); app.use('/wrong-mount', router);
const wrongOrigin: RouterType = unrelated(); wrongOrigin.get('/initializer-wins', handler);
const actual = Router(); actual.get('/actual', handler); app.use('/extra', handler, actual); app.use('/array', [actual]);
