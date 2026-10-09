def first(): pass
from apscheduler.schedulers.blocking import BlockingScheduler
BlockingScheduler = other
scheduler = BlockingScheduler()
scheduler.add_job(first)
