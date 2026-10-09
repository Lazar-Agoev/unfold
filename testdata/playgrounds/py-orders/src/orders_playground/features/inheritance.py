class Root:
    def code(self) -> str:
        return "root"


class Left(Root):
    def code(self) -> str:
        return "left/" + super().code()


class Right(Root):
    def code(self) -> str:
        return "right/" + super().code()


class Combined(Left, Right):
    def describe(self) -> str:
        return self.code()


class ReceiptFormatter:
    def __call__(self, order_id: str) -> str:
        return f"receipt/{order_id}"


def examples() -> tuple[str, str]:
    combined = Combined()
    formatter = ReceiptFormatter()
    return combined.describe(), formatter("demo")
