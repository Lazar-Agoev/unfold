from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
methods = [app.task(name='unique.first')]
methods[0](second)
app.task(name='unique.first', queue='orders')(first)
