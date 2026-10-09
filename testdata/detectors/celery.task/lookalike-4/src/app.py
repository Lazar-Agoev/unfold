def first(): pass
from celery import Celery
app = Celery()
app = other
app.task(name='unique.first', queue='orders')(first)
