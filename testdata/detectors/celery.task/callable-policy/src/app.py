from celery import Celery
app = Celery('demo')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
class Commands:
    @staticmethod
    def run(): pass
alias = first
app.task(name='alias')(alias)
app.task(name='method')(Commands.run)
app.task(name='inline')(lambda: 1)
