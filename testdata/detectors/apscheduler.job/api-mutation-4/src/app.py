from apscheduler.schedulers.blocking import BlockingScheduler
from apscheduler.triggers.cron import CronTrigger
scheduler = BlockingScheduler(timezone='UTC')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
from apscheduler.job import Job
Job._modify = fake
scheduler.add_job(first, CronTrigger.from_crontab('0 2 * * *', timezone='UTC'), id='first')
