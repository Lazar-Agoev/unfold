from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
alias = app
del alias.get
app.get('/safe')(endpoint)
