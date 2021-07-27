# cyrus01337: delete this file and the references to it in ./__init__.py once it performs it's initial run successfully - also keep a copy of the JSON file and remove the original as a precautionary measure
async def migrate_json_to_psql(bot):
    async with bot.pool.acquire() as connection:
        query = "INSERT INTO donators VALUES ($1, $2);"
        rows = []

        for guild_id, user_id in bot.patreon_json.items():
            rows.append(guild_id, user_id)
        await connection.executemany(query, rows)
