from celery import Celery
def first(): pass
app = Celery('demo', loader=Loader)
app.task(name='unique.first', queue='orders')(first)
