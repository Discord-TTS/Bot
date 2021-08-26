from typing import Any, Literal, Union

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


class WSOptionalGenericJSON(TypedDict, total=False):
    t: WS_TARGET

class WSGenericJSON(WSOptionalGenericJSON, TypedDict):
    c: str
    a: Any

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
    info: list[str]
    args: dict[str, dict[str, Any]] # {"run_code": {"code": "print('hello world')"}}
    nonce: str

class WSRequestJSON(TypedDict):
    c: Literal["request"]
    a: WSRequestArgs
    t: WS_TARGET


class WSClientResponseJSON(TypedDict):
    c: Literal["response"]
    a: dict[str, Any]
    t: str
