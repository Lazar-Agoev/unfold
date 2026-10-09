from apscheduler.schedulers.blocking import BlockingScheduler
def first(): pass
scheduler = BlockingScheduler(**settings)
scheduler.add_job(first)
