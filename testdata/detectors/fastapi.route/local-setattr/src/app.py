from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
def setattr(*args): pass
setattr(app, 'get', fake)
app.get('/safe')(endpoint)
