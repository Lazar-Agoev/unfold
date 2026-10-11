from fastapi import FastAPI

app = FastAPI()


@app.get("/admin/stats")
def stats():
    return {}
