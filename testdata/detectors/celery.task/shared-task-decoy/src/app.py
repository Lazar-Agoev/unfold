from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
def shared_task(*args, **kwargs): return lambda fn: fn
shared = shared_task
shared = other
@shared(name='unique.first')
def hidden(): pass
app.task(name='unique.first', queue='orders')(first)
