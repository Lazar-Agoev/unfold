from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
class Meta(type):
    def __getattribute__(cls, name): return fake
class Commands(metaclass=Meta):
    @staticmethod
    def run(): pass
class Child(Commands): pass
alias = Commands.run
app.get('/callback')(alias)
app.get('/dependency', dependencies=[Depends(alias)])(endpoint)
