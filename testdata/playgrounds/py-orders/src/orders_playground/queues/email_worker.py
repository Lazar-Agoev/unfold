from ..jobs.send_email import send_email
from ..lib.retry import with_retry
from .queues import celery_app


@celery_app.task(name="emails.send", queue="emails")
def email_task(payload: dict[str, str]) -> None:
    with_retry(lambda: send_email(payload), 3)
