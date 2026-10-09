from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
state = app.__dict__
state |= {'get': fake}
app.get('/safe')(endpoint)
