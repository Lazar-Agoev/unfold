from apscheduler.schedulers.blocking import BlockingScheduler
from apscheduler.triggers.cron import CronTrigger
scheduler = BlockingScheduler(timezone='UTC')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
class Commands:
    @staticmethod
    def run(): pass
alias = first
scheduler.add_job(alias)
scheduler.add_job(Commands.run)
scheduler.add_job(lambda: 1)
