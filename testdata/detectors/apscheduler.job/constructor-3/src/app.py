from apscheduler.schedulers.blocking import BlockingScheduler
def first(): pass
scheduler = BlockingScheduler(job_defaults={})
scheduler.add_job(first)
