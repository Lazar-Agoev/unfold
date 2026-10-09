from ..jobs.process_order import process_order
from .queues import celery_app

process_order_task = celery_app.task(name="orders.process", queue="orders")(process_order)
