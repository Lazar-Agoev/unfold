class RateLimited(Exception):
    retry_after = 60


def fetch_messages(account):
    return []
