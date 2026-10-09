from celery.app.base import Celery as C
application = C(main='demo', broker=URL, backend='cache+memory://')
