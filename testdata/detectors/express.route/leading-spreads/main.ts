import express, { Router } from 'express';
function first() {} function second() {} function wrap(fn: unknown) { return fn; }
const app = express();
app.get(...args, [first, [second, ...rest]], wrap(first), () => {});
app.post(...args, [], [[]]); app.put(...args);
const router = Router(); app.use('/one', router); app.use('/two', router); app.use(...mountArgs, router);
router.get(...args, first); router.post('/known', second);
