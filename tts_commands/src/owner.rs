use std::{borrow::Cow, fmt::Write, hash::Hash, time::Duration};

use aformat::{ToArrayString, aformat};
use futures_channel::mpsc::UnboundedSender;
use num_format::{Locale, ToFormattedString};
use typesize::TypeSize;

use crate::{REQUIRED_SETUP_PERMISSIONS, REQUIRED_VC_PERMISSIONS};

use self::serenity::{
    CollectComponentInteractions,
    builder::*,
    small_fixed_array::{FixedArray, FixedString},
};
use poise::{CreateReply, serenity_prelude as serenity};

use tts_core::{
    common::{dm_generic, safe_truncate},
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
        todm.tag().into_owned(),
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

    let mut added_role: u64 = 0;
    for member in should_be_ofs_members {
        added_role += 1;
        http.add_member_role(support_guild_id, *member, data.config.ofs_role, None)
            .await?;
    }

    let mut removed_role: u64 = 0;
    for member in should_not_be_ofs_members {
        removed_role += 1;
        http.remove_member_role(support_guild_id, *member, data.config.ofs_role, None)
            .await?;
    }

    ctx.say(
        aformat!("Done! Removed {removed_role} members and added {added_role} members!").as_str(),
    )
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
    info_(ctx).await
}

/// Shows debug information including voice info and database info.
#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn info(ctx: Context<'_>) -> CommandResult {
    info_(ctx).await
}

pub async fn info_(ctx: Context<'_>) -> CommandResult {
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
    ctx.data().leave_vc(guild_id).await.map_err(Into::into)
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

struct Field {
    name: String,
    size: usize,
    value: String,
    is_collection: bool,
}

fn process_cache_info(
    serenity_cache: &serenity::Cache,
    kind: Option<&str>,
    db_info: Option<Vec<typesize::Field>>,
) -> Option<Vec<Field>> {
    let cache_stats = match kind {
        Some("db") => Some(db_info.expect("if kind is db, db_info should be filled")),
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
    };

    let mut fields = Vec::new();
    for field in cache_stats? {
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
        }
    }

    fields.sort_by_key(|field| field.size);
    fields.sort_by_key(|field| field.is_collection);
    fields.reverse();
    Some(fields)
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn cache_info(ctx: Context<'_>, kind: Option<String>) -> CommandResult {
    ctx.defer().await?;

    let db_info = if kind.as_deref() == Some("db") {
        let data = ctx.data();
        Some(vec![
            get_db_info("guild db", &data.guilds_db),
            get_db_info("userinfo db", &data.userinfo_db),
            get_db_info("nickname db", &data.nickname_db),
            get_db_info("user voice db", &data.user_voice_db),
            get_db_info("guild voice db", &data.guild_voice_db),
        ])
    } else {
        None
    };

    let cache = ctx.serenity_context().cache.clone();
    let get_cache_info = move || process_cache_info(&cache, kind.as_deref(), db_info);
    let Some(fields) = tokio::task::spawn_blocking(get_cache_info).await.unwrap() else {
        ctx.say("Unknown cache!").await?;
        return Ok(());
    };

    let embed = CreateEmbed::default()
        .title("Cache Statistics")
        .fields(fields.into_iter().take(25).map(|f| (f.name, f.value, true)));

    ctx.send(CreateReply::default().embed(embed)).await?;
    Ok(())
}

fn filter_channels_by<'a>(
    guild: &'a serenity::Guild,
    bot_member: &'a serenity::Member,
    kind: serenity::ChannelType,
    required_permissions: serenity::Permissions,
) -> impl Iterator<Item = &'a serenity::GuildChannel> + use<'a> {
    guild
        .channels
        .iter()
        .filter(move |c| c.base.kind == kind)
        .filter(move |c| {
            let channel_permissions = guild.user_permissions_in(c, bot_member);
            (required_permissions - channel_permissions).is_empty()
        })
}

fn format_channels<'a>(channels: impl Iterator<Item = &'a serenity::GuildChannel>) -> String {
    let mut out = String::new();
    for channel in channels {
        writeln!(out, "`{}`: {}", channel.id, channel.base.name).unwrap();
        if out.len() >= 1024 {
            break;
        }
    }

    safe_truncate(&mut out, 1024);
    out
}

fn get_runner_channel(
    ctx: &serenity::Context,
    shard_id: serenity::ShardId,
) -> Option<UnboundedSender<serenity::ShardRunnerMessage>> {
    ctx.runners.get(&shard_id).map(|entry| entry.1.clone())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn guild_info(ctx: Context<'_>, guild_id: Option<serenity::GuildId>) -> CommandResult {
    let cache = ctx.cache();
    let Some(guild_id) = guild_id.or(ctx.guild_id()) else {
        ctx.say("Missing guild id!").await?;
        return Ok(());
    };

    let guild_shard_id = guild_id.shard_id(cache.shard_count());

    let title = aformat!("Guild Report for {guild_id}");
    let footer = aformat!("Guild Shard Id: {guild_shard_id}");

    let mut embed = CreateEmbed::new()
        .footer(CreateEmbedFooter::new(&*footer))
        .title(&*title);

    let mut permissions_formatted;
    let mut guild_cached = false;
    if let Some(guild) = cache.guild(guild_id) {
        guild_cached = true;
        if let Some(member) = guild.members.get(&cache.current_user().id) {
            let permissions = guild.member_permissions(member);
            let permissions = if permissions.administrator() {
                "Administrator"
            } else {
                permissions_formatted = permissions.to_string();
                safe_truncate(&mut permissions_formatted, 256);
                &permissions_formatted
            };

            let visible_text_channels = format_channels(filter_channels_by(
                &guild,
                member,
                serenity::ChannelType::Text,
                REQUIRED_SETUP_PERMISSIONS,
            ));

            let usable_voice_channels = format_channels(filter_channels_by(
                &guild,
                member,
                serenity::ChannelType::Voice,
                REQUIRED_VC_PERMISSIONS,
            ));

            embed = embed
                .field("Guild Permissions", permissions, false)
                .field("Visible Text Channels", visible_text_channels, true)
                .field("Usable Voice Channels", usable_voice_channels, true);
        } else {
            embed = embed.description("Guild is cached, but has no bot member");
        }
    }

    if !guild_cached {
        let guild_fetchable = ctx.http().get_guild(guild_id).await.is_ok();

        embed = embed.description(if guild_fetchable {
            "Guild is fetchable, but not in cache"
        } else {
            "Guild is not in cache and not fetchable, is TTS Bot in this guild?"
        });
    }

    let custom_id = uuid::Uuid::now_v7().to_u128_le().to_arraystring();
    let custom_ids = FixedArray::from_vec_trunc(vec![FixedString::from_str_trunc(&custom_id)]);

    let restart_button = CreateButton::new(&*custom_id)
        .style(serenity::ButtonStyle::Danger)
        .label("Restart Shard")
        .emoji('♻');

    let action_row = CreateActionRow::buttons(std::slice::from_ref(&restart_button));
    let components = CreateComponent::ActionRow(action_row);

    let reply = CreateReply::new()
        .embed(embed)
        .components(std::slice::from_ref(&components));

    ctx.send(reply).await?;

    let response = ctx
        .channel_id()
        .collect_component_interactions(ctx.serenity_context())
        .timeout(Duration::from_secs(60 * 5))
        .author_id(ctx.author().id)
        .custom_ids(custom_ids)
        .await;

    let Some(interaction) = response else {
        return Ok(());
    };

    let http = ctx.http();
    let _ = interaction.defer(http).await;

    let shard_id = serenity::ShardId(guild_shard_id);
    let Some(channel) = get_runner_channel(ctx.serenity_context(), shard_id) else {
        let message = CreateInteractionResponseFollowup::new()
            .content("No shard runner found in runners map, cannot restart!");

        interaction.create_followup(http, message).await?;
        return Ok(());
    };

    let restart_msg = serenity::ShardRunnerMessage::Restart;
    if channel.unbounded_send(restart_msg).is_err() {
        let message = CreateInteractionResponseFollowup::new()
            .content("Shard runner channel does not exist anymore");

        interaction.create_followup(http, message).await?;
        return Ok(());
    }

    let message = CreateInteractionResponseFollowup::new()
        .content("Shard has been told to restart, let's see what happens.");

    interaction.create_followup(http, message).await?;

    Ok(())
}

pub fn commands() -> [Command; 7] {
    [
        dm(),
        debug(),
        register(),
        remove_cache(),
        refresh_ofs(),
        cache_info(),
        guild_info(),
    ]
}
