from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import JSONResponse

from ..lib.async_handler import async_handler
from ..lib.slack import slack
from ..queues.queues import celery_app
from ..services.order_service import order_service
from .middleware import require_user, validate_body

orders_router = APIRouter()


def list_orders() -> dict:
    return {"orders": order_service.list()}


async def get_order(order_id: str) -> dict:
    order = order_service.get(order_id)
    if order is None:
        raise HTTPException(status_code=404, detail="Order not found")
    return order


async def create_order(request: Request) -> JSONResponse:
    draft = await request.json()
    if request.headers.get("prefer") == "respond-async":
        job = celery_app.signature("orders.process", args=[draft], queue="orders").delay()
        return JSONResponse({"job_id": job.id}, status_code=202)
    order = order_service.place(draft)
    slack.post("#orders", f"Order {order['id']} placed")
    return JSONResponse(order, status_code=201)


def cancel_order(order_id: str) -> dict:
    order = order_service.cancel(order_id)
    if order is None:
        raise HTTPException(status_code=404, detail="Order not found")
    return order


orders_router.get("")(list_orders)
orders_router.get("/{order_id}")(async_handler(get_order))
orders_router.post("", dependencies=[Depends(require_user), Depends(validate_body)])(create_order)
orders_router.delete("/{order_id}", dependencies=[Depends(require_user)])(cancel_order)
