from typing import Any, Dict, List, Literal, Union

from typing_extensions import TypedDict

__all__ = (
    "WS_TARGET",
    "WSSendJSON",
    "WSKillJSON",
    "WSRequestJSON",
    "WSGenericJSON",
    "WSClientResponseJSON",
)


WS_TARGET = Union[Literal["*", "support"], int]
class EmptyDict(TypedDict): ...


class WSGenericJSON(TypedDict):
    c: str
    a: Any
    t: WS_TARGET

class WSSendJSON(TypedDict):
    c: Literal["send"]
    a: WSGenericJSON
    t: WS_TARGET


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
    t: WS_TARGET


class WSClientResponseJSON(TypedDict):
    c: Literal["response"]
    a: Dict[str, Any]
    t: str
