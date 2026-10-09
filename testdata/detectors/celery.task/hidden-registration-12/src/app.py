from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
class Custom(Celery): pass
other = Custom('other')
other.task(name='unique.first')(second)
app.task(name='unique.first', queue='orders')(first)
