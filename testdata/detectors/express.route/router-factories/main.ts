import make, { Router as CreateRouter, type Router } from 'express';
function handler() {}
const named = CreateRouter(); const member = make.Router(); const constructed = new CreateRouter();
named.get('/named', handler); member.post('/member', handler); constructed.put('/constructed', handler);
function register(router: Router) { router.get('/typed', handler); }
declare const declared: Router;
declared.get('/ambient', handler);
