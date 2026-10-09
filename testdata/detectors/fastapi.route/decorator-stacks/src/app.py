from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
from functools import cache
def wrap(fn): return lambda: 1
@cache
@app.get('/cached')
def cached(): pass
@wrap
@app.get('/outer')
def outer(): pass
@app.get('/inner')
@wrap
def inner(): pass
@app.get('/one')
@app.post('/two')
def stacked(): pass
@wrap
class Commands:
    @app.get('/class')
    def command(self): pass
app.get('/alias')(cached)
app.get('/lambda')(lambda: 1)
app.get('/wrapped-value')(wrap(endpoint))
