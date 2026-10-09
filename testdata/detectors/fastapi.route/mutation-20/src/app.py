from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
arguments = (app, 'get', fake)
setattr(*arguments)
app.get('/safe')(endpoint)
