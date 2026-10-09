from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
other = Celery('other')
app.task(name='unique.first', queue='orders')(first)
other.task(name='unique.second', queue='emails')(second)
