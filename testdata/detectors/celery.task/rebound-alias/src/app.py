from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
alias = first
alias = second
app.task(name='changed')(alias)
