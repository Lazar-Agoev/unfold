from apscheduler.schedulers.blocking import BlockingScheduler
def first(): pass
scheduler = BlockingScheduler({'apscheduler.timezone':'UTC'})
scheduler.add_job(first)
