from booking.calendar import is_free, list_slots, reserve
from booking.errors import NoFreeSlot
from booking.notify import notify_reserved


def reserve_first_free(calendar, day):
    for slot in list_slots(calendar, day):
        if slot.blocked:
            continue
        if is_free(slot):
            reserve(slot)
            break
    else:
        raise NoFreeSlot(day)
    notify_reserved(calendar, slot)
