from apscheduler.schedulers.blocking import BlockingScheduler
from apscheduler.triggers.cron import CronTrigger
scheduler = BlockingScheduler(timezone='UTC')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
class Meta(type): pass
class Commands(metaclass=Meta):
    @staticmethod
    def run(): pass
scheduler.add_job(Commands.run)
