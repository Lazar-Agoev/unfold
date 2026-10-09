def first(): pass
class BlockingScheduler: pass
scheduler = BlockingScheduler()
scheduler.add_job(first)
