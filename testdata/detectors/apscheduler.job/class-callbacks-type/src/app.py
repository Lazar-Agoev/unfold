from apscheduler.schedulers.blocking import BlockingScheduler
from apscheduler.triggers.cron import CronTrigger
scheduler = BlockingScheduler(timezone='UTC')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
class Commands(metaclass=type):
    @staticmethod
    def run(): pass
class Child(Commands): pass
alias = Child.run
scheduler.add_job(Commands.run)
scheduler.add_job(Child.run)
scheduler.add_job(alias)
