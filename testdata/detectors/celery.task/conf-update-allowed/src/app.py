from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
app.conf.update(task_always_eager=FLAG, task_eager_propagates=True, task_serializer='json', accept_content=['json'])
app.task(name='unique.first', queue='orders')(first)
