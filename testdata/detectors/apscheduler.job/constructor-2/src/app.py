from apscheduler.schedulers.blocking import BlockingScheduler
def first(): pass
scheduler = BlockingScheduler(executors=executors)
scheduler.add_job(first)
