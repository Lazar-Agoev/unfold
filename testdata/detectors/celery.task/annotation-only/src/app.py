from celery import Celery
def first(): pass
def install(app: Celery):
    app.task(name='unique.first', queue='orders')(first)
