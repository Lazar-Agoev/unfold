"""UTF-8 / UTF-16 range fixture: café, Ελληνικά, 𐐀, and 📦."""


def κόστος(ποσό: int) -> str:
    return f"📦 café: {ποσό} €"


def 𐐀receipt(value: int) -> str:
    return κόστος(value)


def unicode_example() -> str:
    label = "before 📦 after"
    return label + 𐐀receipt(3)


if __name__ == "__main__":
    print(unicode_example())
