from celery import Celery

from ..config import BROKER_URL, TASK_ALWAYS_EAGER

celery_app = Celery("orders", broker=BROKER_URL, backend="cache+memory://")
celery_app.conf.update(
    task_always_eager=TASK_ALWAYS_EAGER,
    task_eager_propagates=True,
    task_serializer="json",
    accept_content=["json"],
)
