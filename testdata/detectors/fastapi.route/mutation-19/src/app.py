from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
delattr(*(app, 'get'))
app.get('/safe')(endpoint)
