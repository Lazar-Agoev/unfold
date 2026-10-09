from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
from builtins import setattr as write
write(app, 'get', fake)
app.get('/safe')(endpoint)
