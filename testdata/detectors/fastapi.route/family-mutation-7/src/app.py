from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
dictionary = APIRouter.__dict__
dictionary['get'] = fake
app.get('/safe')(endpoint)
