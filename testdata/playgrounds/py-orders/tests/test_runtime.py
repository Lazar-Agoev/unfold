import asyncio
import json
import os
import subprocess
import sys
from pathlib import Path

from orders_playground.features import control, decorators, dynamic, imports, inheritance, unicode
from orders_playground.scheduler import scheduler


def test_scheduler_registrations_and_one_shot_handlers():
    jobs = {job.id: job for job in scheduler.get_jobs()}
    assert set(jobs) == {"cleanup", "metrics"}
    assert all(str(job.trigger.timezone) == "UTC" for job in jobs.values())
    assert str(jobs["cleanup"].trigger) == "cron[month='*', day='*', day_of_week='*', hour='2', minute='0']"
    assert str(jobs["metrics"].trigger) == "cron[month='*', day='*', day_of_week='*', hour='*', minute='*/15']"
    assert jobs["cleanup"].func() == 0
    assert jobs["metrics"].func() == {"pending": 0}


def test_cli_and_legacy_export(tmp_path):
    root = Path(__file__).resolve().parents[1]
    for arguments in [["report"], ["retry"], ["replay"], ["import", str(root / "examples/orders.json")]]:
        result = subprocess.run([sys.executable, "-m", "orders_playground.cli", *arguments], capture_output=True, text=True, timeout=15)
        assert result.returncode == 0, result.stderr
    result = subprocess.run([str(Path(sys.executable).parent / "orders-legacy-export"), str(root / "examples/orders.json")], capture_output=True, text=True, timeout=15)
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout) == [{"id": None, "customer_id": "demo"}]


def test_python_only_examples_execute():
    assert decorators.examples() == ("Invoice MANUAL", "1")
    assert inheritance.examples() == ("left/right/root", "receipt/demo")
    assert imports.examples() == ("public", "public", "namespace", "public")
    assert dynamic.examples() == ("order/123.json", "named/123.json", "order/123", "RECEIPT")
    assert "📦 café: 3 €" in unicode.unicode_example()
    increment = control.counter()
    assert (increment(), increment()) == (1, 2)
    assert list(control.records([" a ", "b"])) == ["A", "B"]
    assert control.scan(["a", "b"])[0] == ["A", "B", "COMPLETE", "a"]
    assert asyncio.run(control.collect()) == ["A", "B"]


def test_monkey_patch_is_confined_and_restored():
    original = dynamic.fallback.public_label
    try:
        assert dynamic.monkey_patch() == "replacement"
    finally:
        dynamic.fallback.public_label = original
