from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
app.signature('unique.first').delay()
app.send_task('unique.first')
app.task(name='unique.first', queue='orders')(first)
