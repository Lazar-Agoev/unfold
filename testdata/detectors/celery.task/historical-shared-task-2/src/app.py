from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
from celery import shared_task as shared
shared = other
@shared(name='unique.first')
def hidden(): pass
app.task(name='unique.first', queue='orders')(first)
