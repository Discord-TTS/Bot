from __future__ import annotations

from datetime import datetime, time, timedelta
from inspect import cleandoc
from typing import TYPE_CHECKING, Awaitable, Dict, List, Tuple

import discord
from discord.ext import tasks

import utils


if TYPE_CHECKING:
    from main import TTSBot


sep = utils.OPTION_SEPERATORS[2]
special_sep = utils.OPTION_SEPERATORS[1]
lookup = {True: "Commands:", False: "Events:"}
get_from_date = "SELECT * FROM analytics WHERE date_collected = $1"
def sleep_until(time: time) -> Awaitable[None]:
    now = datetime.utcnow()
    date = now.date()
    if now.time() > time:
        date += timedelta(days=1)

    then = datetime.combine(date, time)
    return discord.utils.sleep_until(then)


def setup(bot: TTSBot):
    bot.add_cog(Loops(bot))

class Loops(utils.CommonCog):
    yesterday_data = None
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

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

        rows: List[Tuple[str, bool, int]] = []
        for raw_event, count in self.bot.analytics_buffer.items():
            event = utils.removeprefix(raw_event, "on_")
            rows.append((event, raw_event == event, count))

        await self.bot.pool.executemany(query, rows)
        self.bot.analytics_buffer = utils.SafeDict()

    @tasks.loop(minutes=10)
    @utils.decos.handle_errors
    async def send_analytics_msg(self, wait: bool = True):
        if wait:
            midday = time(hour=12)
            await sleep_until(midday)

        max = 0
        yesterday = datetime.today() - timedelta(days=1)
        sections: Dict[str, List[List[str]]] = {
            "Commands:": [], "Events:": []
        }

        embed = discord.Embed(
            title="TTS Bot Analytics",
            colour=utils.NETURAL_COLOUR,
            timestamp=yesterday,
        )

        async with self.bot.pool.acquire() as conn:
            yesterday_data = await conn.fetch(get_from_date, yesterday)
            for row in yesterday_data:
                event, count, is_command, *_ = row
                if len(sections[lookup[is_command]]) >= 10:
                    continue

                max_count: int = (await conn.fetchrow("""
                    SELECT max(count) FROM analytics
                    WHERE event = $1 and is_command = $2
                """, event, is_command))["max"] # type: ignore

                if is_command:
                    event = f"-{event}"

                seperator = special_sep if count == max_count else sep

                first_sect = f"{seperator} `{event}:"
                second_sect = f"{count} (Max: {max_count})`"

                max = max if max > len(first_sect) else len(first_sect)
                sections[lookup[is_command]].append([first_sect, second_sect])

        embed.description = ""
        for section_name in lookup.values():
            embed.description += section_name + "\n" # type: ignore
            for first, second in sections[section_name]:
                embed.description += f"{first:<{max}} {second}\n"

        embed.description += await utils.get_redis_info(self.bot.cache_db)
        await self.bot.channels["analytics"].send(embed=embed)
