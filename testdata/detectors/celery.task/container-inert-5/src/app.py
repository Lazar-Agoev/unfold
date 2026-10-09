from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
def shared_task(*args, **kwargs): return fake
methods = {'task': shared_task}
methods['task'](name='unique.first')(second)
app.task(name='unique.first', queue='orders')(first)
