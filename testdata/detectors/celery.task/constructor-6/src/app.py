from celery import Celery
def first(): pass
app = Celery('demo', **options)
app.task(name='unique.first', queue='orders')(first)
