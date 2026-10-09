from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
router = APIRouter()
@router.get('/g')
def g(): pass
@router.post(path='/p')
async def p(): pass
router.put('/u')(func=endpoint)
router.patch('/a')(endpoint)
router.delete('/d')(endpoint)
router.options('/o')(endpoint)
router.head('/h')(endpoint)
router.api_route('/r')(endpoint)
router.get('/none')()
app.include_router(router, prefix='/v1')
