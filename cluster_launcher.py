"Cluster Launcher for large instances of TTS Bot, contact Gnome!#6669 for help at running TTS Bot at scale."

from __future__ import annotations

import asyncio
import multiprocessing as _multiprocessing
import os
import sys
import time
from configparser import ConfigParser
from functools import partial
from itertools import zip_longest
from signal import SIGHUP, SIGINT, SIGKILL, SIGTERM
from typing import (TYPE_CHECKING, Dict, Iterable, List, Optional, Tuple,
                    TypeVar, Union)

import aiohttp
import requests
import websockets

import utils

config = ConfigParser()
config.read("config.ini")

if TYPE_CHECKING:
    _T = TypeVar("_T")
    _CLUSTER_ARG = Tuple[int, int, Tuple[int]]
    _CLUSTER_RET = Tuple[int, int, _CLUSTER_ARG]
    _WSSP = websockets.WebSocketServerProtocol

def group_by(iterable: Iterable[_T], by:int) -> Iterable[Tuple[_T]]:
    yield from zip_longest(*[iter(iterable)]*by)


def make_user_agent():
    first = "DiscordBot (https://github.com/Gnome-py/Discord-TTS-Bot Rolling)"
    versions = "Python/{sysver} requests/{requestsver}".format(
        sysver=".".join(str(i) for i in sys.version_info[:3]),
        requestsver=requests.__version__
    )

    return f"{first} {versions}"

def fetch_num_shards() -> int:
    response = requests.get(
        "https://discord.com/api/v7/gateway/bot",
        headers={
            "Authorization": "Bot " + config["Main"]["Token"],
            "User-Agent": make_user_agent()
        }
    )
    response.raise_for_status()
    return response.json()["shards"]

def run_bot(cluster_id: int, total_shard_count: int, shards: List[int]) -> _CLUSTER_RET:
    """This function is run from the bot process"""
    import asyncio
    import sys

    import utils
    from main import main

    try:
        return_code = asyncio.run(main(cluster_id, total_shard_count, shards))
    except:
        return_code = utils.DO_NOT_RESTART_CLUSTER
    finally:
        sys.exit(return_code)

class ClusterManager:
    def __init__(self, websocket_host: str, websocket_port: int) -> None:
        self.websocket_port = websocket_port
        self.websocket_host = websocket_host

        self.shutting_down: bool = False
        self.loop = asyncio.get_running_loop()
        self.monitors: Dict[int, asyncio.Task] = {}
        self.processes: Dict[int, Optional[int]] = {}
        self.websockets: Dict[int, websockets.WebSocketServerProtocol] = {}

        for sig in (SIGTERM, SIGINT, SIGHUP):
            callable_handler = partial(self.signal_handler, sig)
            self.loop.add_signal_handler(sig, callable_handler)

    async def __aenter__(self):
        await self.start()
        return self

    async def __aexit__(self, *_, **__):
        if not self.shutting_down:
            await self.shutdown()


    def signal_handler(self, signal: int, *args, **kwargs):
        logger.debug(f"Signal {signal} received")
        return self.loop.create_task(self.shutdown())

    def cluster_watcher(self, cluster_args: _CLUSTER_ARG):
        cluster_id = cluster_args[0]
        cluster_name = f"Cluster {cluster_id}"

        process = multiprocessing.Process(target=run_bot, args=cluster_args)
        while not self.shutting_down:
            process.start() # start bot
            self.processes[cluster_id] = process.pid # store PID so can be killed later
            process.join() # wait until finished

            while process.exitcode is None:
                logger.warning(f"{cluster_name} joined with no exit code, rejoining and waiting 10 seconds.")
                process.join()
                time.sleep(10)

            if process.exitcode in (utils.KILL_EVERYTHING, utils.DO_NOT_RESTART_CLUSTER):
                logger.warning(f"Shutting down all clusters due to {cluster_name} return.")

                process.close()
                del self.processes[cluster_id]
                return asyncio.run_coroutine_threadsafe(self.shutdown(), self.loop)

            elif process.exitcode == utils.RESTART_CLUSTER:
                logger.warning(f"Restarting {cluster_name} due to RESTART_CLUSTER")

                process.close()
                process = multiprocessing.Process(target=run_bot, args=cluster_args)

            else:
                if any(256-process.exitcode == sig for sig in (SIGTERM, SIGINT, SIGKILL)):
                    break # process was killed and already sent log for it

                logger.error(f"{cluster_name} returned unknown value: {process.exitcode}, restarting it.")

                process.close()
                process = multiprocessing.Process(target=run_bot, args=cluster_args)


    async def start(self):
        shards_per_cluster = int(config["Clustering"]["shards_per_cluster"])
        shard_count = int(config["Clustering"].get("shard_count") or fetch_num_shards())

        full_clusters, last_cluster_shards = divmod(shard_count, shards_per_cluster)
        cluster_count = full_clusters + int(bool(last_cluster_shards))

        logger.info(f"Launching {cluster_count} clusters to handle {shard_count} shards with {shards_per_cluster} per cluster.")

        all_shards = enumerate(group_by(range(shard_count), shards_per_cluster))
        for cluster_id, shards in all_shards:
            args = (cluster_id, shard_count, shards)
            cluster_watcher_func = partial(self.cluster_watcher, args)
            self.monitors[cluster_id] = asyncio.Task(utils.to_thread(cluster_watcher_func))

        async def keep_alive():
            async with websockets.serve(
                self.websocket_handler,
                self.websocket_host,
                self.websocket_port
            ):
                await asyncio.gather(*self.monitors.values())

        self.keep_alive = self.loop.create_task(keep_alive())

    async def shutdown(self, *args, **kwargs):
        if self.shutting_down:
            return

        self.shutting_down = True
        logger.warning("Shutting all clusters down")
        for cluster_id, pid in self.processes.items():
            if pid:
                logger.debug(f"Killing cluster {cluster_id}")
                try:
                    await self.websockets[cluster_id].send("CLOSE")
                except Exception as err:
                    logger.error(err)
                else:
                    logger.debug(f"Sent signal to cluster {cluster_id}")
            else:
                logger.warning(f"Cluster {cluster_id} has no PID")

        logger.info("Kiilled all processes, and cancelling keep alive. Bye!")
        self.keep_alive.cancel()

        try:
            await asyncio.wait_for(self.keep_alive, timeout=3)
        except asyncio.TimeoutError:
            logger.error("Timed out on shutdown, force killing!")
            sys.exit()


    async def websocket_handler(self, connection: _WSSP, cluster: str):
        cluster_id = int("".join(c for c in cluster if c.isdigit()))
        logger.debug(f"New Websocket connection from cluster {cluster_id}")

        self.websockets[cluster_id] = connection
        async for msg in connection:
            if isinstance(msg, bytes):
                msg = msg.decode()

            logger.debug(f"Recieved msg from cluster {cluster_id}: {msg}")
            command, *arguments = msg.lower().split()

            handler = getattr(self, f"{command}_handler", None)
            if handler is None:
                logger.error(f"Websocket sent unknown command: {command}")
                continue

            await handler(connection, *arguments)

    async def send_handler(self, _: _WSSP, cluster: str, *msg: str):
        await self.websockets[int(cluster)].send(msg)

    async def broadcast_handler(self, _: _WSSP, *args: str):
        if args[0].lower() == "change_log_level":
            logger.setLevel(args[1].upper())
            for handler in logger.handlers:
                handler.setLevel(args[1].upper())

        for connection in self.websockets.values():
            await connection.send(" ".join(args))

    async def kill_handler(self, _: _WSSP,
        to_kill: str,
        signal: Union[int, str] = SIGTERM
    ):
        process = self.processes.get(int(to_kill))
        if not process:
            return

        os.kill(process, int(signal))
        return True


async def main():
    global logger

    host = config["Clustering"].get("websocket_host", "localhost")
    port = int(config["Clustering"].get("websocket_port", "8765"))

    async with aiohttp.ClientSession() as session:
        logger = utils.setup_logging(aio=True, level=config["Main"]["log_level"], session=session, prefix="`[Launcher]`: ")
        async with ClusterManager(host, port) as manager:
            try:
                await manager.keep_alive
            except asyncio.CancelledError:
                pass

        await asyncio.sleep(1) # wait for final logs to be sent

if __name__ == "__main__":
    # I just spent 1 and a half hours trying to figure out why multiprocessing
    # + asyncio decides to mangle return codes of processes, the solution...
    # swap multiprocessing from fork to spawn mode.

    multiprocessing = _multiprocessing.get_context("spawn")
    asyncio.run(main())
