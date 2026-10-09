from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
@app.task()
def generated(): pass
app.task(name='app.generated', queue='emails')(second)
