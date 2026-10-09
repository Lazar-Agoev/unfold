from celery import Celery
def first(): pass
app = Celery('demo', tasks={})
app.task(name='unique.first', queue='orders')(first)
