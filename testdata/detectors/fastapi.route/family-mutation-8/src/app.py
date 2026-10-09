from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
dictionary = FastAPI.__dict__
mutate = dictionary.update
mutate({'get': fake})
app.get('/safe')(endpoint)
