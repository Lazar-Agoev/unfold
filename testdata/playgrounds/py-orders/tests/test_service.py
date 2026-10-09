import json
from datetime import datetime, timedelta, timezone

import pytest
from fastapi.testclient import TestClient

from orders_playground.jobs.cleanup import cleanup_expired_orders
from orders_playground.jobs.import_orders import import_orders
from orders_playground.jobs.retry_failed import retry_failed
from orders_playground.lib.retry import with_retry
from orders_playground.lib.validate import validate_order
from orders_playground.queues.email_worker import email_task
from orders_playground.queues.orders_worker import process_order_task
from orders_playground.queues.report_worker import report_task
from orders_playground.server import app
from orders_playground.services.order_service import order_service


def test_health_routes_and_dependency_rejection(draft):
    with TestClient(app) as client:
        assert client.get("/health").json() == {"status": "ok"}
        assert client.get("/health/live").json() == {"status": "alive"}
        assert client.get("/v1/status").json() == {"version": "1", "orders": 0}
        assert client.post("/orders", json=draft).status_code == 401
        assert client.post("/orders", headers={"x-user": "alice"}, json={}).status_code == 422
        assert client.get("/orders/missing").status_code == 404


def test_order_api_lifecycle_and_wrapped_handler(draft):
    with TestClient(app) as client:
        created = client.post("/orders", headers={"x-user": "alice"}, json=draft)
        assert created.status_code == 201
        order = created.json()
        assert order["status"] == "pending"
        assert client.get(f"/orders/{order['id']}").json() == order
        assert client.get("/orders").json() == {"orders": [order]}
        assert client.get("/admin/stats").json() == {"orders": 1, "pending": 1}
        assert client.delete(f"/orders/{order['id']}").status_code == 401
        cancelled = client.delete(f"/orders/{order['id']}", headers={"x-user": "alice"})
        assert cancelled.json()["status"] == "cancelled"
        assert client.delete("/orders/missing", headers={"x-user": "alice"}).status_code == 404


def test_async_order_queues_once_in_eager_mode(draft):
    with TestClient(app) as client:
        response = client.post("/orders", headers={"x-user": "alice", "prefer": "respond-async"}, json=draft)
        assert response.status_code == 202
        assert response.json()["job_id"]
        assert order_service.count == 1


def test_celery_eager_tasks(draft):
    order = process_order_task.delay(draft).get()
    assert order["customer_id"] == "alice"
    assert email_task.delay({"to": draft["email"], "subject": "Receipt"}).get() is None
    assert report_task.delay().get() == {"orders": 1}
    retry_failed()


def test_cleanup_only_removes_expired_unpaid(draft):
    recent = order_service.place(draft)
    expired = order_service.place(draft)
    old = order_service.place(draft)
    paid = order_service.place(draft)
    expired["created_at"] = (datetime.now(timezone.utc) - timedelta(days=2)).isoformat()
    old["created_at"] = (datetime.now(timezone.utc) - timedelta(days=8)).isoformat()
    paid["created_at"] = old["created_at"]
    paid["status"] = "paid"
    assert cleanup_expired_orders() == 1
    assert order_service.get(old["id"]) is None
    assert expired["status"] == "cancelled"
    assert recent["status"] == "pending"
    assert paid["status"] == "paid"


def test_import_reports_partial_failures(tmp_path, draft):
    path = tmp_path / "orders.json"
    path.write_text(json.dumps([draft]))
    assert import_orders(str(path)) == {"imported": 1, "rejected": 0}
    path.write_text(json.dumps([draft, {}]))
    with pytest.raises(ExceptionGroup) as failure:
        import_orders(str(path))
    assert len(failure.value.exceptions) == 1
    assert order_service.count == 2


def test_retry_retries_then_returns_and_preserves_final_error():
    attempts = []
    def eventually():
        attempts.append(1)
        if len(attempts) < 3:
            raise ValueError("temporary")
        return "ok"
    assert with_retry(eventually, 3) == "ok"
    assert len(attempts) == 3
    with pytest.raises(ValueError, match="attempts"):
        with_retry(eventually, 0)
    with pytest.raises(ZeroDivisionError):
        with_retry(lambda: 1 / 0, 2)


@pytest.mark.parametrize("invalid", [None, {}, {"customer_id": "a", "email": "wrong", "items": []}, {"customer_id": "a", "email": "a@b", "items": [{"sku": "X", "quantity": True, "price": 1}]}])
def test_validation_rejects_invalid_orders(invalid):
    with pytest.raises(ValueError):
        validate_order(invalid)
