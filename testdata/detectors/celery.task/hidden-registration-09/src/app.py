from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
app.autodiscover_tasks(['provider'])
app.task(name='unique.first', queue='orders')(first)
