from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
app.get(b'/bytes')(endpoint)
router = APIRouter(prefix=b'/bytes')
router.get('/route')(endpoint)
app.include_router(router, prefix='/api')
second = APIRouter()
second.get('/other')(endpoint)
app.include_router(second, prefix=b'/bytes')
