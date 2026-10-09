from collections.abc import AsyncIterator, Iterator
from contextlib import asynccontextmanager
from io import StringIO


def normalize(value: str) -> str:
    return value.strip().upper()


def records(values: list[str]) -> Iterator[str]:
    for value in values:
        yield normalize(value)


def scan(values: list[str]) -> tuple[list[str], dict[str, str], set[str], tuple[str, ...]]:
    cleaned = [normalize(value) for value in values if (length := len(value)) > 0]
    names = {value: normalize(value) for value in values}
    unique = {normalize(value) for value in values}
    streamed = tuple(normalize(value) for value in values)
    for value in cleaned:
        if value == "STOP":
            break
        if value == "SKIP":
            continue
    else:
        cleaned.append("COMPLETE")
    try:
        first = cleaned[0]
    except IndexError:
        cleaned.append("EMPTY")
    else:
        cleaned.append(first.lower())
    finally:
        names["state"] = "done"
    with StringIO("a\nb") as source:
        names["first"] = source.readline().strip()
    return cleaned, names, unique, streamed


def counter():
    count = 0
    def increment() -> int:
        nonlocal count
        count += 1
        return count
    return increment


async def async_records() -> AsyncIterator[str]:
    for value in ["a", "b"]:
        yield normalize(value)


@asynccontextmanager
async def resource() -> AsyncIterator[dict[str, bool]]:
    state = {"open": True}
    try:
        yield state
    finally:
        state["open"] = False


async def collect() -> list[str]:
    results = []
    async with resource() as state:
        async for value in async_records():
            if state["open"]:
                results.append(value)
    return results
