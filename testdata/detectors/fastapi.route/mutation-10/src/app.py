from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
delegated = app.router
delegated = other
delegated.get = fake
app.get('/safe')(endpoint)
