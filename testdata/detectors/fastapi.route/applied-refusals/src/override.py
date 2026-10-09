from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
class Base:
    @app.get('/original')
    def named(self): pass
class Child(Base):
    def named(self): pass
def register(value: Base):
    app.get('/applied')(value.named)
