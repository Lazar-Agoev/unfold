from exports import scheduler as clock
from apscheduler.triggers.cron import CronTrigger as Cron
def first(): pass
def second(): pass
clock.add_job(first, Cron.from_crontab('0 2 * * *'), id='a')
clock.add_job(trigger=Cron.from_crontab(timezone='UTC', expr='*/15 * * * *'), id='b', func=second)
