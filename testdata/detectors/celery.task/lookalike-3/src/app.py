def first(): pass
from celery import Celery
class Custom(Celery): pass
app = Custom()
app.task(name='unique.first', queue='orders')(first)
