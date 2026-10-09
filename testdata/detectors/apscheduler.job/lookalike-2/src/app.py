def first(): pass
from apscheduler.schedulers.blocking import BlockingScheduler
class Custom(BlockingScheduler): pass
scheduler = Custom()
scheduler.add_job(first)
