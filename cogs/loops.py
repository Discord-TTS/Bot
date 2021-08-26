from __future__ import annotations

import datetime
from typing import TYPE_CHECKING

import discord
from discord.ext import tasks

import utils

if TYPE_CHECKING:
    from main import TTSBot


sep = utils.OPTION_SEPERATORS[1]
special_sep = utils.OPTION_SEPERATORS[2]
lookup = {True: "Commands:", False: "Events:"}
get_events = """
    SELECT * FROM analytics
    WHERE date_collected = $1 AND NOT is_command
    ORDER BY count DESC
    LIMIT 10
"""
get_commands = """
    SELECT * FROM analytics
    WHERE date_collected = $1 AND is_command
    ORDER BY count DESC
    LIMIT 10
"""


def setup(bot: TTSBot):
    bot.add_cog(Loops(bot))

class Loops(utils.CommonCog):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

        self.run_once = False
        # This is a sin, adds all tasks to self.tasks and starts them
        attrs = (getattr(self, str_attr) for str_attr in dir(self))
        self.tasks = [attr for attr in attrs if isinstance(attr, tasks.Loop)]
        for task in self.tasks:
            task.before_loop(self.bot.wait_until_ready)
            task.start()

    def cog_unload(self):
        for task in self.tasks:
            task.cancel()


    @tasks.loop(seconds=60)
    @utils.decos.handle_errors
    async def send_info_to_db(self):
        query = """
            INSERT INTO analytics(event, is_command, count)
            VALUES($1, $2, $3)

            ON CONFLICT ON CONSTRAINT analytics_pkey
            DO UPDATE SET count = analytics.count + EXCLUDED.count
        ;"""

        rows: list[tuple[str, bool, int]] = []
        for raw_event, count in self.bot.analytics_buffer.items():
            event = raw_event.removeprefix("on_")
            rows.append((event, raw_event == event, count))

        await self.bot.pool.executemany(query, rows)
        self.bot.analytics_buffer = utils.SafeDict()

    @tasks.loop(time=datetime.time(hour=12))
    @utils.decos.handle_errors
    async def send_analytics_msg(self):
        if self.bot.cluster_id not in {None, 0}:
            return self.send_analytics_msg.cancel()

        if not self.run_once:
            self.run_once = True
            return

        max_len = 0
        yesterday = datetime.datetime.today() - datetime.timedelta(days=1)
        sections: dict[str, list[list[str]]] = {"Commands:": [], "Events:": []}

        embed = discord.Embed(
            title="TTS Bot Analytics",
            colour=utils.NETURAL_COLOUR,
            timestamp=yesterday,
        )

        async with self.bot.pool.acquire() as conn:
            yesterday_events = await conn.fetch(get_events, yesterday)
            yesterday_commands = await conn.fetch(get_commands, yesterday)

            for row in [*yesterday_events, *yesterday_commands]:
                event, count, is_command, *_ = row
                max_count: int = (await conn.fetchrow("""
                    SELECT max(count) FROM analytics
                    WHERE event = $1 and is_command = $2
                """, event, is_command))["max"] # type: ignore

                if is_command:
                    event = f"-{event}"

                seperator = special_sep if count == max_count else sep

                first_sect = f"{seperator} `{event}:"
                second_sect = f"{count} (Max: {max_count})`"

                max_len = max(max_len, len(first_sect))
                sections[lookup[is_command]].append([first_sect, second_sect])

        embed.description = ""
        for section_name in lookup.values():
            embed.description += section_name + "\n" # type: ignore
            for first, second in sections[section_name]:
                embed.description += f"{first:<{max_len}} {second}\n"

        embed.description += await utils.get_redis_info(self.bot.cache_db)
        await self.bot.channels["analytics"].send(embed=embed)
