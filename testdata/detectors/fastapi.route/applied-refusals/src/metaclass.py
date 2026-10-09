from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
class Meta(type):
    def __getattribute__(self, name): return fake
class Commands(metaclass=Meta):
    @staticmethod
    @app.get('/original')
    def named(): pass
app.get('/applied')(Commands.named)
