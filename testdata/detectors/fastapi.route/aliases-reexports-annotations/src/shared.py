from fastapi import APIRouter as Router
router = Router(prefix='/own')
def handler(): pass
router.get('/x')(handler)
