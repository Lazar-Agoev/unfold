def first(): pass
class Celery: pass
app = Celery()
app.task(name='unique.first', queue='orders')(first)
