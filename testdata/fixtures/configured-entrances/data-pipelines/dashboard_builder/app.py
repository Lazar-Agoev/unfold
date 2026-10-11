from fastapi import FastAPI

from dashboard_builder.router import router

app = FastAPI()
app.include_router(router, prefix="/dashboard-builder")


@app.get("/health")
def health():
    return "ok"
