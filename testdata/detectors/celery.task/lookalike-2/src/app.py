def first(): pass
from celery import Celery
Celery = other
app = Celery()
app.task(name='unique.first', queue='orders')(first)
