from fastapi import APIRouter, FastAPI

router = APIRouter()


@router.get("/items")
def items():
    return []


public_app = FastAPI()
admin_app = FastAPI()
public_app.include_router(router, prefix="/public")
admin_app.include_router(router, prefix="/admin")


def main():
    return public_app
