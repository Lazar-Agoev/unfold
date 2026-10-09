from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
from celery.app.task import Task as T
T.__call__ = fake
app.task(name='unique.first', queue='orders')(first)
