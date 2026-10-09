from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
other = Celery('other')
other.conf.update(task_annotations={'*': {'rate_limit':'1/m'}})
other.task(name='unique.first')(second)
app.task(name='unique.first', queue='orders')(first)
