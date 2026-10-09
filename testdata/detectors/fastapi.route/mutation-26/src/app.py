from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
def patch(parameter: FastAPI):
    parameter.get = fake
app.get('/safe')(endpoint)
