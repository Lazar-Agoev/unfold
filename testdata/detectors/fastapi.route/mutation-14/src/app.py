from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
with context as app.get: pass
app.get('/safe')(endpoint)
