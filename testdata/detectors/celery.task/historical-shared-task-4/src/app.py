from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
from celery import shared_task
class Box: pass
box = Box()
box.shared = shared_task
box.shared = other
@box.shared(name='unique.first')
def hidden(): pass
app.task(name='unique.first', queue='orders')(first)
