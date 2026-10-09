import express, { Router } from 'express';
function handler() {} const app = express(); const router = Router();
app.use('/outer', router); router.get('/outer-route', handler);
function nested() { const router = Router(); router.get('/inner', handler); }
const empty = Router(); app.use(empty); empty.get('', handler); empty.post('/', handler);
const dynamic = Router(); app.use('/a', dynamic); app.use('/b', dynamic); dynamic.get(path, handler);
const bare = Router(); bare.get('', handler);
