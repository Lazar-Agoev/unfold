from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
app.task(name='literal', queue='orders')(first)
app.task(name='missing')(first)
app.task(name='dynamic', queue=QUEUE)(first)
app.task(name='bytes', queue=b'orders')(first)
app.task(name='empty', queue='')(first)
app.task(name='interpolated', queue=f'{QUEUE}')(first)
