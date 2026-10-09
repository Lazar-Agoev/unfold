from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
APIRouter.add_api_route = fake
app.get('/safe')(endpoint)
