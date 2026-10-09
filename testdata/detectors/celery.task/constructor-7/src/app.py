from celery import Celery
def first(): pass
app = Celery('demo', 'loader')
app.task(name='unique.first', queue='orders')(first)
