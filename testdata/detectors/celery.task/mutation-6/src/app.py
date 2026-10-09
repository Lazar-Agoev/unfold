from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
from celery import Task
Alias = Task
Alias = other
Alias.bind = fake
app.task(name='unique.first', queue='orders')(first)
