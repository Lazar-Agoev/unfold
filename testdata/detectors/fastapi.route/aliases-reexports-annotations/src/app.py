import fastapi as api
from reexport import exported
app = api.FastAPI()
app.include_router(exported, prefix='/api')
def install(application: api.FastAPI):
    @application.head('/health')
    def health(): pass
