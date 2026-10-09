from time import sleep

from ..lib.logger import logger

outbox: list[dict[str, str]] = []


def send_email(payload: dict[str, str]) -> None:
    if "@" not in payload["to"]:
        raise ValueError("Email recipient is invalid")
    outbox.append(payload)
    sleep(0.001)
    logger.info("Email delivered", payload)
    outbox.remove(payload)


def flush_outbox() -> None:
    for payload in list(outbox):
        send_email(payload)
