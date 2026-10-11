from fastapi import FastAPI

from costs import compute

app = FastAPI()


@app.post("/cost/compute")
def compute_cost():
    return compute()
