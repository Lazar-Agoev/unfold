from fastapi.applications import FastAPI as Parent
class Custom(Parent):
    def get(self, *args, **kwargs): return unknown
