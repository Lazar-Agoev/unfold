from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
methods = {'task': app.task}
expanded = {**methods}
expanded['task'](second, name='unique.first')
app.task(name='unique.first', queue='orders')(first)
