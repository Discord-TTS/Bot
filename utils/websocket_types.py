import uuid
from typing import Any, Dict, List, Literal, Union

from typing_extensions import TypedDict


_TARGET = Union[Literal["*"], int]
class EmptyDict(TypedDict): ...


class WSGenericJSON(TypedDict):
    c: str
    a: Any
    t: Union[_TARGET, uuid.UUID]

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


# Classes for future state flow
class WSClientRequestArgs(TypedDict):
    info: List[str]
    nonce: uuid.UUID

class WSServerRequestArgs(TypedDict):
    info: List[str]


class WSClientRequestJSON(TypedDict):
    c: Literal["request"]
    a: WSClientRequestArgs
    t: _TARGET

class WSServerRequestJSON(TypedDict):
    c: Literal["request"]
    a: WSServerRequestArgs
    t: uuid.UUID


class WSClientResponseJSON(TypedDict):
    c: Literal["response"]
    a: Dict[str, Any]
    t: uuid.UUID

class WSServerResponseJSON(TypedDict):
    c: Literal["response"]
    a: List[Dict[str, Any]]
    t: uuid.UUID
