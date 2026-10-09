from apscheduler.schedulers.blocking import BlockingScheduler
from apscheduler.triggers.cron import CronTrigger
scheduler = BlockingScheduler(timezone='UTC')
def first(): pass
def second(): pass
def fake(*args, **kwargs): pass
scheduler.add_job(first, CronTrigger.from_crontab('0 2 * * *'), id='job0')
scheduler.add_job(first, CronTrigger.from_crontab(expr=' 0  2 * * * ', timezone='UTC'), id='job1')
scheduler.add_job(first, CronTrigger.from_crontab('0 2 * * *', 'UTC'), id='job2')
scheduler.add_job(first, CronTrigger.from_crontab(EXPR), id='job3')
scheduler.add_job(first, CronTrigger.from_crontab(b'0 2 * * *'), id='job4')
scheduler.add_job(first, CronTrigger.from_crontab('0 2 * *'), id='job5')
scheduler.add_job(first, CronTrigger.from_crontab('0 2 * * * *'), id='job6')
scheduler.add_job(first, CronTrigger.from_crontab('0 2 * * *', **options), id='job7')
scheduler.add_job(first, CronTrigger.from_crontab('0 2 * * *', expr='* * * * *'), id='job8')
scheduler.add_job(first, CronTrigger(hour=2), id='job9')
scheduler.add_job(first, 'cron', id='job10')
scheduler.add_job(first, unknown_trigger, id='job11')
