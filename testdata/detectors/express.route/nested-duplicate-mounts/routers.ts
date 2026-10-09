import express, { Router } from 'express';
export const parent = Router(); export const child = express.Router();
function handler() {} child.get('/item//', handler); child.post('/', handler);
parent.use('/nested/', child);