from .config import REPORT_QUEUE
from .lib.pool import Worker
from .queues import email_worker, orders_worker, report_worker
from .queues.queues import celery_app
from .services.order_service import order_service


def purge() -> None:
    for order in order_service.list():
        if order["status"] == "cancelled":
            order_service.remove(order["id"])


maintenance = Worker("cleanup-pool", purge)


def main() -> None:
    maintenance.run()
    celery_app.worker_main(["worker", "--loglevel=INFO", "--pool=solo", "-Q", f"orders,emails,{REPORT_QUEUE}"])


if __name__ == "__main__":
    main()
