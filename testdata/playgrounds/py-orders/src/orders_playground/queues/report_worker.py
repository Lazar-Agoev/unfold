from ..config import REPORT_QUEUE
from ..jobs.report import print_report
from .queues import celery_app

report_task = celery_app.task(name="reports.build", queue=REPORT_QUEUE)(print_report)
