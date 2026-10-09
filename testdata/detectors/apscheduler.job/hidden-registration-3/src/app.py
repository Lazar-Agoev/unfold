from apscheduler.schedulers.blocking import BlockingScheduler
from apscheduler.triggers.cron import CronTrigger
scheduler = BlockingScheduler(timezone='UTC')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
scheduler.add_job(first, CronTrigger.from_crontab('0 2 * * *', timezone='UTC'), id='first')
class Box: pass
box = Box()
box.register = scheduler.add_job
box.register(second, id='first')
