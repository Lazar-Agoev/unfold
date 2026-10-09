from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
@app.get('/original')
def named(): pass
alias = named
alias = fake
app.get('/applied')(alias)
