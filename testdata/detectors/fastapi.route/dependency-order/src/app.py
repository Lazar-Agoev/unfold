from fastapi import FastAPI, APIRouter, Depends as D
def application(): pass
def mounting(): pass
def router_dep(): pass
def first(): pass
def second(): pass
def endpoint(): pass
app = FastAPI(dependencies=[D(application)])
router = APIRouter(dependencies=[D(router_dep)])
router.post('/x', dependencies=[D(first), D(dependency=second)])(endpoint)
app.include_router(router, dependencies=[D(mounting)])
