from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
def Depends(value): return value
app.get('/x', dependencies=[Depends(endpoint)])(endpoint)
