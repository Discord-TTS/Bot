from typing import Any, Dict, List, Literal, Union

from typing_extensions import TypedDict

__all__ = (
    "_TARGET",
    "WSSendJSON",
    "WSKillJSON",
    "WSRequestJSON",
    "WSGenericJSON",
    "WSClientResponseJSON",
)


_TARGET = Union[Literal["*", "support"], int]
class EmptyDict(TypedDict): ...


class WSGenericJSON(TypedDict):
    c: str
    a: Any
    t: _TARGET

class WSSendJSON(TypedDict):
    c: Literal["send"]
    a: WSGenericJSON
    t: _TARGET


class WSKillArgs(TypedDict):
    signal: int

class WSKillJSON(TypedDict):
    c: Literal["kill"]
    a: Union[WSKillArgs, EmptyDict]
    t: int


class WSRequestArgs(TypedDict):
    info: List[str]
    nonce: str

class WSRequestJSON(TypedDict):
    c: Literal["request"]
    a: WSRequestArgs
    t: _TARGET


class WSClientResponseJSON(TypedDict):
    c: Literal["response"]
    a: Dict[str, Any]
    t: str
