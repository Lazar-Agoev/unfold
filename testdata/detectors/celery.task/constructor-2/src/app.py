from celery import Celery
def first(): pass
app = Celery('demo', config_source=settings)
app.task(name='unique.first', queue='orders')(first)
