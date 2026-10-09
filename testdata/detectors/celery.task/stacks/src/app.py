from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
def wrap(fn): return lambda: 0
@wrap
@app.task(name='outer', queue='orders')
def outer(): pass
@app.task(name='inner', queue='orders')
@wrap
def inner(): pass
@wrap
class Commands:
    @app.task(name='class', queue='orders')
    def command(self): pass
@app.task(name='original', queue='orders')
def original(): pass
app.task(name='again', queue='orders')(original)
