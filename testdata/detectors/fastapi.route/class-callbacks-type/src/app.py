from fastapi import FastAPI, APIRouter, Depends
app = FastAPI()
def endpoint(): pass
def fake(*args, **kwargs): pass
class Commands(metaclass=type):
    @staticmethod
    def run(): pass
class Child(Commands): pass
alias = Child.run
app.get('/callback')(Commands.run)
app.get('/dependency', dependencies=[Depends(Child.run), Depends(alias)])(endpoint)
