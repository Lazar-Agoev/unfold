from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
app.__dict__.__ior__({'get': fake})
app.get('/safe')(endpoint)
