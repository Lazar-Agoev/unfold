class PaymentError(Exception):
    pass


class CardDeclined(PaymentError):
    def __init__(self, intent: str) -> None:
        super().__init__(f"card declined for {intent}")
        self.intent = intent
