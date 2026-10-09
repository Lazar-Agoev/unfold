from apscheduler.schedulers.blocking import BlockingScheduler
from apscheduler.triggers.cron import CronTrigger
scheduler = BlockingScheduler(timezone='UTC')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
class Meta(type):
    def __getattribute__(cls, name):
        return second
class Commands(metaclass=Meta):
    @staticmethod
    def run(): pass
callback = Commands.run
scheduler.add_job(callback)
