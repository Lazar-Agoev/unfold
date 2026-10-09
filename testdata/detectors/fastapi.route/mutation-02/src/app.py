from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
alias = app
alias.get = fake
app.get('/safe')(endpoint)
