from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
def wrap(value): return fake
class Meta(type): pass
@wrap
class Commands(metaclass=Meta):
    @staticmethod
    def run(): pass
app.get('/callback')(Commands.run)
app.get('/dependency', dependencies=[Depends(Commands.run)])(endpoint)
