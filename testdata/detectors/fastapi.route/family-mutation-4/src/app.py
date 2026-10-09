from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
setattr(APIRouter, 'include_router', fake)
app.get('/safe')(endpoint)
