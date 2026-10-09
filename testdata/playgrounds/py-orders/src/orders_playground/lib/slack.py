from .logger import logger


class Slack:
    def post(self, channel: str, message: str) -> None:
        logger.info(channel, message)


slack = Slack()
