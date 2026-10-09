from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
from celery import shared_task
@shared_task(name='outside')
def shared(): pass
@app.task
def bare(): pass
app.task(first, name='direct')
app.signature('name').delay()
first.delay()
first.apply_async()
