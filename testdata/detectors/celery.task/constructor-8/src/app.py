from celery import Celery
def first(): pass
app = Celery('demo', main='duplicate')
app.task(name='unique.first', queue='orders')(first)
