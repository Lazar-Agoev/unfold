from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
Celery.autodiscover_tasks(app, ['provider'])
app.task(name='unique.first', queue='orders')(first)
