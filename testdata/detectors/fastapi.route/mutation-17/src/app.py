from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
from builtins import delattr as erase
erase(app, 'get')
app.get('/safe')(endpoint)
