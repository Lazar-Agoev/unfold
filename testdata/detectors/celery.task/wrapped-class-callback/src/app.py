from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
def wrap(value): return fake
class Meta(type): pass
@wrap
class Commands(metaclass=Meta):
    @staticmethod
    def run(): pass
app.task(name='unique.wrapped')(Commands.run)
