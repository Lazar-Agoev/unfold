from fastapi import FastAPI, APIRouter
def endpoint(): pass
def fake(*args, **kwargs): pass
def register(receiver: FastAPI):
    receiver.get('/annotated')(endpoint)
app = FastAPI()
app.get('/concrete')(endpoint)
