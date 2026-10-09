from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
def register(application: FastAPI):
    application.get('/annotated')(endpoint)
app.get('/safe')(endpoint)
