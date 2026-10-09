import express, { type Express, type Application } from 'express';
function handler() {}
const app = express();
app.get('/get', handler); app.post('/post', handler); app.put('/put', handler); app.patch('/patch', handler);
app.delete('/delete', handler); app.options('/options', handler); app.head('/head', handler); app.all('/all', handler);
function register(value: Express, other: Application) { value.get('/typed', handler); other.post('/application', handler); }
(app as Application).get(('/wrapped' as string), (handler)!);
app.get(`/template`, handler); app.get(dynamicPath, handler); app.get('', handler);
