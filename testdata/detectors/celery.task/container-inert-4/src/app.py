from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
methods = [fake]
methods[0](second, name='unique.first')
app.task(name='unique.first', queue='orders')(first)
