def endpoint(): pass
from fastapi import FastAPI
class Sub(FastAPI): pass
app = Sub()
app.get('/safe')(endpoint)
