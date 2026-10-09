from fastapi import FastAPI

from ..services.order_service import order_service


def admin_stats() -> dict[str, int]:
    orders = order_service.list()
    return {"orders": order_service.count, "pending": sum(order["status"] == "pending" for order in orders)}


def register_admin_routes(app: FastAPI) -> None:
    app.get("/admin/stats")(admin_stats)
