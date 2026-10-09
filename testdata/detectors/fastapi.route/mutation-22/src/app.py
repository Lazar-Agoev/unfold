from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
dict.update(app.__dict__, {'get': fake})
app.get('/safe')(endpoint)
