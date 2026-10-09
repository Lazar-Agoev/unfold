from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
update = app.conf.update
update(task_serializer='json')
app.task(name='unique.first', queue='orders')(first)
