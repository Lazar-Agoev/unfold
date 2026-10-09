from apscheduler.schedulers.blocking import BlockingScheduler
from apscheduler.triggers.cron import CronTrigger
scheduler = BlockingScheduler(timezone='UTC')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
@scheduler.scheduled_job('cron', hour=2)
def hidden(): pass
from apscheduler import AsyncScheduler
async_scheduler = AsyncScheduler()
async_scheduler.add_schedule(first, 'cron')
