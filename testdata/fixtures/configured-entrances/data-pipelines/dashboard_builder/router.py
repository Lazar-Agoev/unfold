from fastapi import APIRouter

router = APIRouter()


@router.get("/model/{model}")
def model(model: str):
    return model


@router.post("/explore")
def explore():
    return {}
