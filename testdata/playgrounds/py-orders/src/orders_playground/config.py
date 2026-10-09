import os

PORT = int(os.environ.get("PORT", "8000"))
API_VERSION = os.environ.get("API_VERSION", "1")
BROKER_URL = os.environ.get("BROKER_URL", "redis://127.0.0.1:6379/0")
REPORT_QUEUE = os.environ.get("REPORT_QUEUE", "reports")
TASK_ALWAYS_EAGER = os.environ.get("ORDERS_EAGER", "0") == "1"
