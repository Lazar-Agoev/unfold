from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
app.get('/x', path='/y')(endpoint)
app.get('/x', **settings)(endpoint)
app.get('/x')(*handlers)
app.get('/x')(endpoint, func=fake)
app.get('/x')(endpoint, fake)
app.get('/x')(func=endpoint, unexpected=fake)
