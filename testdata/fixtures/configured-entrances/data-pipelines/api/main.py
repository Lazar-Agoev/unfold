from fastapi import FastAPI

from dashboard_builder.router import router
from shared import tokens

app = FastAPI()
app.include_router(router, prefix="/dashboard-builder")


@app.post("/runs")
def create_run():
    return tokens.current()


@app.get("/runs/{run_id}")
def get_run(run_id: str):
    return run_id
