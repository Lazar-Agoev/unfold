from apscheduler.schedulers.blocking import BlockingScheduler
def first(): pass
def install(scheduler: BlockingScheduler):
    scheduler.add_job(first)
