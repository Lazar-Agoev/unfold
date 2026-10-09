from fastapi import APIRouter

health_router = APIRouter()
health_router.get("/live")(lambda: {"status": "alive"})
