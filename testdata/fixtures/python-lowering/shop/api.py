from fastapi import APIRouter, Depends, FastAPI

from shop.auth import current_user
from shop.service import place_order

app = FastAPI()
router = APIRouter(prefix="/orders")


@router.post("/{order_id}")
def create_order(order_id: str, user=Depends(current_user)):
    return place_order(order_id)


app.include_router(router)
