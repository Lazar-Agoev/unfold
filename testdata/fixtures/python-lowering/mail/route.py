from mail.classify import classify, is_relevant
from mail.delivery import deliver
from mail.labels import label_for
from mail.profiles import cached_profile, fetch_profile
from mail.routing import Routed


def route_message(message, override):
    profile = cached_profile(message.sender) or fetch_profile(message.sender)
    decision = Routed(override) if override else classify(message, profile)
    labels = [label_for(part) for part in message.parts if is_relevant(part)]
    return deliver(decision, labels)
