from apscheduler.schedulers.blocking import BlockingScheduler
from apscheduler.triggers.cron import CronTrigger

from .jobs.cleanup import cleanup_expired_orders
from .jobs.report import print_report, report_metrics
from .lib.calendar import schedule as schedule_reminder

scheduler = BlockingScheduler(timezone="UTC")
scheduler.add_job(cleanup_expired_orders, CronTrigger.from_crontab("0 2 * * *", timezone="UTC"), id="cleanup")
scheduler.add_job(lambda: report_metrics(), CronTrigger.from_crontab("*/15 * * * *", timezone="UTC"), id="metrics")


def main() -> None:
    schedule_reminder("scheduler startup report", print_report)
    scheduler.start()


if __name__ == "__main__":
    main()
