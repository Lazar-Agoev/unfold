from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
class Meta(type):
    def __getattribute__(cls, name): return second
class Commands(metaclass=Meta):
    @staticmethod
    def run(): pass
class Child(Commands): pass
alias = Commands.run
app.task(name='unique.callback')(Child.run)
