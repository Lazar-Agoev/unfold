from typing import TYPE_CHECKING as checking

if checking:
    from .literal_exports import public as annotation_only

TYPE_CHECKING = True
if TYPE_CHECKING:
    from .literal_exports import public as runtime_candidate

runtime_candidate()
