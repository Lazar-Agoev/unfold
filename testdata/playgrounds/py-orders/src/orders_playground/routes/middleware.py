from fastapi import Header, HTTPException, Request

from ..lib.validate import validate_order


def require_user(x_user: str | None = Header(default=None)) -> str:
    if not x_user:
        raise HTTPException(status_code=401, detail="x-user header required")
    return x_user


async def validate_body(request: Request) -> None:
    try:
        validate_order(await request.json())
    except ValueError as error:
        raise HTTPException(status_code=422, detail=str(error)) from error
