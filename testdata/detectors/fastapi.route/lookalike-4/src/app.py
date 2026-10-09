def endpoint(): pass
from fastapi import FastAPI
class Sub(FastAPI): pass
def run(app: Sub):
    app.get('/x')(endpoint)
app.get('/safe')(endpoint)
