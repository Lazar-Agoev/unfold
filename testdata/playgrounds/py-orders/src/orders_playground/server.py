import signal

import uvicorn
from fastapi import FastAPI

from .config import API_VERSION, PORT
from .lib.logger import logger
from .routes.admin import register_admin_routes
from .routes.health import health_router
from .routes.orders import orders_router
from .services.order_service import order_service

app = FastAPI(title="Python orders playground")


@app.get("/health")
def health_check() -> dict[str, str]:
    return {"status": "ok"}


@app.get(f"/v{API_VERSION}/status")
def status() -> dict:
    return {"version": API_VERSION, "orders": order_service.count}


app.include_router(orders_router, prefix="/orders")
app.include_router(health_router, prefix="/health")
register_admin_routes(app)


def shutdown(signum: int = 0, frame: object = None) -> None:
    logger.info("HTTP server stopped", {"signal": signum})


def main() -> None:
    signal.signal(signal.SIGTERM, shutdown)
    uvicorn.run(app, host="127.0.0.1", port=PORT)


if __name__ == "__main__":
    main()
