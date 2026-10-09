from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
app.config_from_object(settings)
app.task(name='unique.first', queue='orders')(first)
