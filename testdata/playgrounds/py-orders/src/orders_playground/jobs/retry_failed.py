from ..lib.retry import with_retry
from .send_email import flush_outbox


def retry_failed() -> None:
    with_retry(flush_outbox, 3)
