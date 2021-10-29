from __future__ import annotations


import traceback
from typing import TYPE_CHECKING, Any, Coroutine, Protocol, Union

import discord
import orjson
import websockets
from discord.ext import commands, tasks
from discord.utils import maybe_coroutine

import utils

if TYPE_CHECKING:
    from main import TTSBotPremium
    from utils.websocket_types import WSGenericJSON

    class ClusteredTTSBot(TTSBotPremium):
        websocket: websockets.WebSocketClientProtocol

    class RequestDataFunction(Protocol):
        def __call__(self, b: ClusteredTTSBot, *_) -> Union[utils.JSON_IN, Coroutine[Any, Any, utils.JSON_IN]]: ...


async def code_runner(b: ClusteredTTSBot, code: str, *_):
    try:
        executor = AsyncCodeExecutor(code, arg_dict={"_bot": b})
        return "\n".join([repr(result) async for result in executor if result is not None])
    except Exception as error:
        return f"```{''.join(traceback.format_exception(type(error), error, error.__traceback__))}```"

data_lookup: dict[str, RequestDataFunction] = {
    "run_code": code_runner,
    "ping": lambda *_: "pong",
    "guild_count":  lambda b, *_: len(b.guilds),
    "voice_count":  lambda b, *_: len(b.voice_clients),
    "member_count": lambda b, *_: sum(guild.member_count for guild in b.guilds),
    "has_support":  lambda b, *_: None if b.get_support_server() is None else b.cluster_id,
}

WHITE_CHECK_MARK = "\U00002705"
def setup(bot: ClusteredTTSBot):
    if bot.cluster_id is None:
        return

    global codeblock_converter, AsyncCodeExecutor, Scope

    from jishaku.codeblocks import codeblock_converter
    from jishaku.repl import AsyncCodeExecutor, Scope
    bot.add_cog(Clustering(bot))

class Clustering(utils.CommonCog, command_attrs={"hidden": True}):
    def __init__(self, bot: ClusteredTTSBot):
        super().__init__(bot)

        self.bot = bot
        self.websocket_client.start()

    @tasks.loop()
    @utils.decos.handle_errors
    async def websocket_client(self):
        try:
            async for msg in self.bot.websocket:
                wsjson: WSGenericJSON = orjson.loads(msg)
                self.bot.dispatch("websocket_msg", msg)

                args = [*wsjson["a"].values()]
                if wsjson.get("t", None):
                    args.append(wsjson["t"])

                command = wsjson["c"].lower()
                self.bot.dispatch(command, *args)
        except websockets.ConnectionClosed as error:
            disconnect_msg = f"Websocket disconnected with code `{error.code}: {error.reason}`"
            try:
                self.bot.websocket = await self.bot.create_websocket()
            except Exception as new_error:
                self.bot.logger.error(f"{disconnect_msg} and failed to reconnect: {new_error}")
                await self.bot.close(utils.RESTART_CLUSTER)
            else:
                self.bot.logger.warning(f"{disconnect_msg} and was able to reconnect!")

    @commands.command()
    @commands.is_owner()
    async def run_code_as(self, ctx: utils.TypedContext, cluster_id: int, *, code: str):
        response = await ctx.request_ws_data(
            "run_code",
            target=cluster_id,
            args={"run_code": {"code": codeblock_converter(code).content}},
        )
        if response is None:
            return

        return_value: str = response[0]["run_code"]
        if return_value:
            await ctx.send(return_value)
        else:
            await ctx.message.add_reaction(WHITE_CHECK_MARK)

    # IPC events that have been plugged into bot.dispatch
    @commands.Cog.listener()
    async def on_websocket_msg(self, msg: str):
        self.bot.logger.debug(f"Recieved Websocket message: {msg}")

    @commands.Cog.listener()
    async def on_close(self, *_):
        await self.bot.close(utils.KILL_EVERYTHING)

    @commands.Cog.listener()
    async def on_restart(self, *_):
        await self.bot.close(utils.RESTART_CLUSTER)

    @commands.Cog.listener()
    async def on_reload(self, cog: str, *_):
        self.bot.reload_extension(cog)

    @commands.Cog.listener()
    async def on_view_load(self, traceback: str, message_id: int, *_):
        view = utils.ShowTracebackView(f"```\n{traceback}```")
        self.bot.add_view(view, message_id=message_id)

    @commands.Cog.listener()
    async def on_change_log_level(self, level: str, *_):
        level = level.upper()
        self.bot.logger.setLevel(level)
        for handler in self.bot.logger.handlers:
            handler.setLevel(level)

    @commands.Cog.listener()
    async def on_request(self, info: list[str], nonce: str, args: dict[str, dict[str, Any]] = {}, *_):
        returns: dict[str, utils.JSON_IN] = {}
        for key in info:
            func, kwargs = data_lookup[key], args.get(key, {})
            returns[key] = await maybe_coroutine(func, self.bot, **kwargs)

        wsjson = utils.data_to_ws_json("RESPONSE", target=nonce, **returns)
        await self.bot.websocket.send(wsjson)

    @commands.Cog.listener()
    async def on_ofs_add(self, owner_id: int, *_):
        support_server = self.bot.get_support_server()
        if support_server is None:
            return

        role = support_server.get_role(703307566654160969)
        if not role:
            return

        try:
            owner_member = await support_server.fetch_member(owner_id)
        except discord.HTTPException:
            return

        await owner_member.add_roles(role)
        self.bot.logger.info(f"Added OFS role to {owner_member}")
