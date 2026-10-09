from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
class Box: pass
box = Box()
box.factory = app.task(name='unique.first')
box.factory(second)
app.task(name='unique.first', queue='orders')(first)
