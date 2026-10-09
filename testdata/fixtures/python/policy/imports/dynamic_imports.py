import importlib as genuine_importlib
from importlib import import_module as genuine_import_module


class Importer:
    def import_module(self, name):
        return name


def import_module(name):
    return name


importlib = Importer()
genuine_importlib.import_module("imports.literal_exports")
genuine_import_module("imports.literal_exports")
importlib.import_module("imports.literal_exports")
import_module("imports.literal_exports")


def dynamic_name(name):
    return genuine_import_module(name)
