from inbox.gmail import RateLimited, fetch_messages
from inbox.locks import mailbox_lock
from inbox.notify import notify_synced
from inbox.retry import schedule_retry
from inbox.store import record_attempt, store_messages


def sync_inbox(account):
    """Pull new mail for one account and store it."""
    with mailbox_lock(account):
        try:
            messages = fetch_messages(account)
        except RateLimited as error:
            schedule_retry(account, error.retry_after)
            return 0
        else:
            store_messages(account, messages)
        finally:
            record_attempt(account)
    notify_synced(account)
    return len(messages)
