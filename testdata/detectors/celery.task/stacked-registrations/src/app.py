from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
@app.task(name='outer')
@app.task(name='inner')
def stacked(): pass
