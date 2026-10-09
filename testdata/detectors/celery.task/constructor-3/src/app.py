from celery import Celery
def first(): pass
app = Celery('demo', changes={})
app.task(name='unique.first', queue='orders')(first)
