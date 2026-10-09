"""Positive and negative written-receiver/MRO cases, never executed."""
from abc import abstractmethod
from typing import final

class Repository:
    def save(self):
        return 1

class Service:
    repo: Repository
    def __init__(self, repo: Repository):
        self.repo = repo
    def run(self):
        return self.repo.save()

class ConflictingService:
    def __init__(self):
        self.repo = Repository()
    def replace(self, other):
        self.repo = other
    def run(self):
        return self.repo.save()

class Base:
    def method(self):
        return 1
    def dispatch(self):
        return self.method()

class Child(Base):
    def method(self):
        return 2

class Abstract:
    @abstractmethod
    def method(self): ...
    def dispatch(self):
        return self.method()

class Callable:
    def __call__(self):
        return 1

instance = Callable()
instance()

class Root:
    def method(self):
        return 1
class Left(Root):
    def method(self):
        return super().method()
class Right(Root):
    def method(self):
        return super().method()
class Diamond(Left, Right):
    def dispatch(self):
        return self.method()

@final
class Fixed:
    @staticmethod
    def clean(value):
        return value
    @classmethod
    def make(cls, value):
        return cls.clean(value)
