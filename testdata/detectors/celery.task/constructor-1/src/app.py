from celery import Celery
def first(): pass
app = Celery('demo', task_cls=Other)
app.task(name='unique.first', queue='orders')(first)
