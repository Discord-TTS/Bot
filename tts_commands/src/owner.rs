// Discord TTS Bot
// Copyright (C) 2021-Present David Thomas
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::{borrow::Cow, hash::Hash};

use self::serenity::{builder::*, small_fixed_array::FixedString};
use num_format::{Locale, ToFormattedString};
use poise::{serenity_prelude as serenity, CreateReply};
use typesize::TypeSize;

use tts_core::{
    common::dm_generic,
    database,
    database_models::Compact,
    structs::{Command, CommandResult, Context, PrefixContext, TTSModeChoice},
};

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn register(ctx: Context<'_>) -> CommandResult {
    poise::samples::register_application_commands(ctx, true).await?;
    Ok(())
}

#[poise::command(prefix_command, hide_in_help, owners_only)]
pub async fn dm(
    ctx: PrefixContext<'_>,
    todm: serenity::User,
    #[rest] message: FixedString<u16>,
) -> CommandResult {
    let attachment_url = ctx.msg.attachments.first().map(|a| a.url.as_str());
    let (content, embed) = dm_generic(
        ctx.serenity_context(),
        &ctx.msg.author,
        todm.id,
        todm.tag(),
        attachment_url,
        &message,
    )
    .await?;

    ctx.msg
        .channel_id
        .send_message(
            ctx.http(),
            CreateMessage::default()
                .content(content)
                .add_embed(CreateEmbed::from(embed)),
        )
        .await?;

    Ok(())
}

#[poise::command(
    prefix_command,
    owners_only,
    hide_in_help,
    aliases("invalidate_cache", "delete_cache"),
    subcommands("guild", "user", "guild_voice", "user_voice")
)]
pub async fn remove_cache(ctx: Context<'_>) -> CommandResult {
    ctx.say("Please run a subcommand!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn guild(ctx: Context<'_>, guild: i64) -> CommandResult {
    ctx.data().guilds_db.invalidate_cache(&guild);
    ctx.say("Done!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn user(ctx: Context<'_>, user: i64) -> CommandResult {
    ctx.data().userinfo_db.invalidate_cache(&user);
    ctx.say("Done!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn guild_voice(ctx: Context<'_>, guild: i64, mode: TTSModeChoice) -> CommandResult {
    ctx.data()
        .guild_voice_db
        .invalidate_cache(&(guild, mode.into()));
    ctx.say("Done!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn user_voice(ctx: Context<'_>, user: i64, mode: TTSModeChoice) -> CommandResult {
    ctx.data()
        .user_voice_db
        .invalidate_cache(&(user, mode.into()));
    ctx.say("Done!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn refresh_ofs(ctx: Context<'_>) -> CommandResult {
    let data = ctx.data();
    let http = &ctx.http();
    let cache = &ctx.cache();

    let support_guild_id = data.config.main_server;
    let support_guild_members = support_guild_id.members(http, None, None).await?;

    let all_guild_owners = cache
        .guilds()
        .iter()
        .filter_map(|id| cache.guild(*id).map(|g| g.owner_id))
        .collect::<Vec<_>>();

    let current_ofs_members = support_guild_members
        .iter()
        .filter(|m| m.roles.contains(&data.config.ofs_role))
        .map(|m| m.user.id)
        .collect::<Vec<_>>();

    let should_not_be_ofs_members = current_ofs_members
        .iter()
        .filter(|ofs_member| !all_guild_owners.contains(ofs_member));
    let should_be_ofs_members = all_guild_owners.iter().filter(|owner| {
        (!current_ofs_members.contains(owner))
            && support_guild_members.iter().any(|m| m.user.id == **owner)
    });

    let mut added_role = 0;
    for member in should_be_ofs_members {
        added_role += 1;
        http.add_member_role(support_guild_id, *member, data.config.ofs_role, None)
            .await?;
    }

    let mut removed_role = 0;
    for member in should_not_be_ofs_members {
        removed_role += 1;
        http.remove_member_role(support_guild_id, *member, data.config.ofs_role, None)
            .await?;
    }

    ctx.say(format!(
        "Done! Removed {removed_role} members and added {added_role} members!"
    ))
    .await?;
    Ok(())
}

/// Debug commands for the bot
#[poise::command(
    prefix_command,
    slash_command,
    guild_only,
    subcommands("info", "leave")
)]
pub async fn debug(ctx: Context<'_>) -> CommandResult {
    _info(ctx).await
}

/// Shows debug information including voice info and database info.
#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn info(ctx: Context<'_>) -> CommandResult {
    _info(ctx).await
}

pub async fn _info(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let guild_id_db: i64 = guild_id.into();

    let data = ctx.data();
    let author_id = ctx.author().id.into();

    let shard_id = ctx.serenity_context().shard_id;
    let user_row = data.userinfo_db.get(author_id).await?;
    let guild_row = data.guilds_db.get(guild_id_db).await?;
    let nick_row = data.nickname_db.get([guild_id_db, author_id]).await?;
    let guild_voice_row = data
        .guild_voice_db
        .get((guild_id_db, guild_row.voice_mode))
        .await?;

    let user_voice_row = data
        .user_voice_db
        .get((author_id, user_row.voice_mode.unwrap_or_default()))
        .await?;

    let voice_client = data.songbird.get(guild_id);
    let embed = CreateEmbed::default()
        .title("TTS Bot Debug Info")
        .description(format!(
            "
Shard ID: `{shard_id}`
Voice Client: `{voice_client:?}`

Server Data: `{guild_row:?}`
User Data: `{user_row:?}`
Nickname Data: `{nick_row:?}`
User Voice Data: `{user_voice_row:?}`
Guild Voice Data: `{guild_voice_row:?}`
"
        ));

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Force leaves the voice channel in the current server to bypass buggy states
#[poise::command(prefix_command, guild_only, hide_in_help)]
pub async fn leave(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    ctx.data()
        .songbird
        .remove(guild_id)
        .await
        .map_err(Into::into)
}

fn get_db_info<CacheKey, RowT>(
    name: &'static str,
    handler: &database::Handler<CacheKey, RowT>,
) -> typesize::Field
where
    CacheKey: Eq + Hash + TypeSize,
    RowT::Compacted: TypeSize,
    RowT: Compact,
{
    typesize::Field {
        name,
        size: handler.get_size(),
        collection_items: handler.get_collection_item_count(),
    }
}

fn guild_iter(cache: &serenity::Cache) -> impl Iterator<Item = serenity::GuildRef<'_>> {
    cache.guilds().into_iter().filter_map(|id| cache.guild(id))
}

fn details_iter<'a>(
    iter: impl Iterator<Item = &'a (impl typesize::TypeSize + 'a)>,
) -> Vec<Vec<typesize::Field>> {
    iter.map(TypeSize::get_size_details).collect::<Vec<_>>()
}

fn average_details(iter: impl Iterator<Item = Vec<typesize::Field>>) -> Vec<typesize::Field> {
    let mut i = 1;
    let summed_details = iter.fold(Vec::new(), |mut avg_details, details| {
        if avg_details.is_empty() {
            return details;
        }

        // get_size_details should return the same amount of fields every time
        assert_eq!(avg_details.len(), details.len());

        i += 1;
        for (avg, cur) in avg_details.iter_mut().zip(details) {
            avg.size += cur.size;
            if let Some(collection_items) = &mut avg.collection_items {
                *collection_items += cur.collection_items.unwrap();
            }
        }

        avg_details
    });

    let details = summed_details.into_iter().map(move |mut field| {
        if let Some(collection_items) = &mut field.collection_items {
            *collection_items /= i;
        }
        field.size /= i;
        field
    });

    details.collect()
}

#[poise::command(prefix_command, owners_only)]
pub async fn cache_info(ctx: Context<'_>, kind: Option<String>) -> CommandResult {
    struct Field {
        name: String,
        size: usize,
        value: String,
        is_collection: bool,
    }

    let serenity_cache = ctx.cache();
    let cache_stats = {
        let data = ctx.data();
        match kind.as_deref() {
            Some("db") => Some(vec![
                get_db_info("guild db", &data.guilds_db),
                get_db_info("userinfo db", &data.userinfo_db),
                get_db_info("nickname db", &data.nickname_db),
                get_db_info("user voice db", &data.user_voice_db),
                get_db_info("guild voice db", &data.guild_voice_db),
            ]),
            Some("guild") => Some(average_details(
                guild_iter(serenity_cache).map(|g| g.get_size_details()),
            )),
            Some("channel") => Some(average_details(
                guild_iter(serenity_cache).flat_map(|g| details_iter(g.channels.iter())),
            )),
            Some("role") => Some(average_details(
                guild_iter(serenity_cache).flat_map(|g| details_iter(g.roles.iter())),
            )),
            Some(_) => None,
            None => Some(serenity_cache.get_size_details()),
        }
    };

    let Some(cache_stats) = cache_stats else {
        ctx.say("Unknown cache!").await?;
        return Ok(());
    };

    let mut fields = Vec::new();
    for field in cache_stats {
        let name = format!("`{}`", field.name);
        let size = field.size.to_formatted_string(&Locale::en);
        if let Some(count) = field.collection_items {
            let (count, size_per) = if count == 0 {
                (Cow::Borrowed("0"), Cow::Borrowed("N/A"))
            } else {
                let count_fmt = count.to_formatted_string(&Locale::en);
                let mut size_per = (field.size / count).to_formatted_string(&Locale::en);
                size_per.push('b');

                (Cow::Owned(count_fmt), Cow::Owned(size_per))
            };

            fields.push(Field {
                name,
                size: field.size,
                is_collection: true,
                value: format!("Size: `{size}b`\nCount: `{count}`\nSize per model: `{size_per}`"),
            });
        } else {
            fields.push(Field {
                name,
                size: field.size,
                is_collection: false,
                value: format!("Size: `{size}b`"),
            });
        };
    }

    fields.sort_by_key(|field| field.size);
    fields.sort_by_key(|field| field.is_collection);
    fields.reverse();

    let embed = CreateEmbed::default()
        .title("Cache Statistics")
        .fields(fields.into_iter().take(25).map(|f| (f.name, f.value, true)));

    ctx.send(CreateReply::default().embed(embed)).await?;
    Ok(())
}

pub fn commands() -> [Command; 6] {
    [
        dm(),
        debug(),
        register(),
        remove_cache(),
        refresh_ofs(),
        cache_info(),
    ]
}
