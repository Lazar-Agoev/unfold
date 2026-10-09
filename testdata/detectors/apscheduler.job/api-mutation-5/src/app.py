from apscheduler.schedulers.blocking import BlockingScheduler
from apscheduler.triggers.cron import CronTrigger
scheduler = BlockingScheduler(timezone='UTC')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
class Box: pass
box = Box()
box.api = scheduler
box.api.add_job = fake
scheduler.add_job(first, CronTrigger.from_crontab('0 2 * * *', timezone='UTC'), id='first')
