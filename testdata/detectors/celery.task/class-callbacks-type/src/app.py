from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
class Commands(metaclass=type):
    @staticmethod
    def run(): pass
class Child(Commands): pass
alias = Child.run
app.task(name='unique.direct')(Commands.run)
app.task(name='unique.inherited')(Child.run)
app.task(name='unique.alias')(alias)
