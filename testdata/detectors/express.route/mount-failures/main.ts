import express, { Router } from 'express';
function handler() {} const app = express(); const orphan = Router(); const child = Router(); const parent = Router();
orphan.get('/orphan//', handler); child.get('/local//', handler);
app.use('/known', child); app.use(dynamicPrefix, child); parent.use('/incomplete', child);
const cycleA = Router(); const cycleB = Router(); cycleA.use('/a', cycleB); cycleB.use('/b', cycleA);
cycleA.get('/cycle', handler); app.use('/escape', cycleA);
