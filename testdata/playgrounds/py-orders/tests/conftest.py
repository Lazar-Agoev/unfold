import os

import pytest

os.environ["ORDERS_EAGER"] = "1"
os.environ["BROKER_URL"] = "memory://"


@pytest.fixture(autouse=True)
def empty_orders():
    from orders_playground.services.order_service import order_service
    from orders_playground.jobs.send_email import outbox

    order_service.repo.orders.clear()
    outbox.clear()
    yield
    order_service.repo.orders.clear()
    outbox.clear()


@pytest.fixture
def draft():
    return {"customer_id": "alice", "email": "alice@example.test", "items": [{"sku": "BOOK", "quantity": 2, "price": 12.5}]}
