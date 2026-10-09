from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
Celery.config_from_object(app, settings)
app.task(name='unique.first', queue='orders')(first)
