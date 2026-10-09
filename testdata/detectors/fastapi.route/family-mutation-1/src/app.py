from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
FastAPI.get = fake
app.get('/safe')(endpoint)
