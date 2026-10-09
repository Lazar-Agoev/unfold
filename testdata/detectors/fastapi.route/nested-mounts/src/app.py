from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
second = FastAPI()
outer = APIRouter(prefix='/outer')
inner = APIRouter(prefix='/inner')
inner.get('//leaf/')(endpoint)
outer.include_router(inner, prefix='/nested')
app.include_router(outer, prefix='/first')
app.include_router(outer, prefix='/first')
second.include_router(outer, prefix='/second')
