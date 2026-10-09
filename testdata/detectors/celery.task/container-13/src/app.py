from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
from celery import shared_task
methods = (shared_task,)
alias = methods
alias[0](name='unique.first')(second)
app.task(name='unique.first', queue='orders')(first)
