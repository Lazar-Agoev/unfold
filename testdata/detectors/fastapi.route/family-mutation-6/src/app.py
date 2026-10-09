from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
from fastapi.applications import FastAPI as F
del F.get
app.get('/safe')(endpoint)
