from apscheduler.schedulers.blocking import BlockingScheduler
def first(): pass
scheduler = BlockingScheduler(jobstores=stores)
scheduler.add_job(first)
