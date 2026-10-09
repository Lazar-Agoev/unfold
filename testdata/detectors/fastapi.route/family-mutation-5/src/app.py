from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
from fastapi.routing import APIRouter as R
R.get = fake
app.get('/safe')(endpoint)
