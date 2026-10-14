"""Capturing one message: two try statements, a conditional expression with calls, and nested
decisions, shaped like a real collector."""

import logging

from pipeline.errors import CaptureError
from pipeline.fetch import execute, read_message
from pipeline.routing import Routed, route
from pipeline.status import capture_failed
from pipeline.store import insert_email, outcome_columns, row_for, store_files, update_expecting

logger = logging.getLogger(__name__)


def bind_context(decision):
    pass


def routing_text(decision):
    return str(decision)


def capture(run, message_id, override):
    """Capture one message: fetch it, route it, and store it."""
    try:
        raw = execute(run.service.users().messages().get(id=message_id))
        message = read_message(raw)
    except Exception as exc:
        run.failures.append(str(exc))
        logger.exception("fetch failed")
        return None
    logger.info("captured %s", message.headers.get("subject"))
    decision = Routed(override) if override else route(message)
    if isinstance(decision, Routed):
        bind_context(decision)
    names = []
    try:
        names = store_files(run, message)
        outcome = outcome_columns(decision)
    except CaptureError:
        capture_failed(run, routing_text(decision))
        raise
    row = row_for(message, outcome, names)
    if run.exact and row["status"] in ("stored", "skipped"):
        run.errors.append("unexpected status")
    if not run.write:
        run.record(row)
        return row
    if (stored := run.lookup(message_id)) is None:
        row_id, inserted = insert_email(row)
        if not inserted and row_id is not None:
            logger.info("duplicate %s", row_id)
            return row
        if row_id is not None:
            logger.info("stored %s", row_id)
        else:
            logger.warning("stored without an id")
    else:
        if not update_expecting(stored, row):
            logger.info("unchanged")
            return row
    run.record(row)
    return row
