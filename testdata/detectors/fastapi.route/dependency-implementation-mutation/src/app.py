from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
from fastapi.params import Depends as Implementation
Implementation.__init__ = fake
app.get('/x', dependencies=[Depends(endpoint)])(endpoint)
