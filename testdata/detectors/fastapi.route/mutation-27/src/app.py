from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
delegated = app.router
alias0 = delegated
alias1 = alias0
alias2 = alias1
alias3 = alias2
alias4 = alias3
alias5 = alias4
alias6 = alias5
alias7 = alias6
alias8 = alias7
alias9 = alias8
alias10 = alias9
alias11 = alias10
alias11.get = fake
app.get('/safe')(endpoint)
