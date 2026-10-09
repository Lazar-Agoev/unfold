from exports import application as app
from functools import cache
@cache
@app.task(name='unique.decorated', queue='orders')
def decorated(): pass
def ordinary(): pass
alias = ordinary
app.task(name='unique.alias', queue='emails')(alias)
app.task(name='unique.keyword', queue='reports')(fun=ordinary)
