from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
class Meta(type): pass
class Commands(metaclass=Meta):
    @app.task(name='method')
    def command(self): pass
