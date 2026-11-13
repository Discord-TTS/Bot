use std::collections::VecDeque;
use std::num::NonZeroU16;

use extract_map::entry::Entry;

use super::{Cache, CacheUpdate};
use crate::model::prelude::*;

impl CacheUpdate for ChannelCreateEvent {
    type Output = MaybeObfuscated;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        let mut guild = cache.guilds.get_mut(&self.channel.guild_id())?;
        match &self.channel {
            MaybeObfuscated::Viewable(guild_channel) => {
                guild.viewable_channels.insert(guild_channel.clone()).map(Into::into)
            },
            MaybeObfuscated::Obfuscated(_) => None,
        }
    }
}

impl CacheUpdate for ChannelDeleteEvent {
    type Output = VecDeque<Message>;

    fn update(&self, cache: &Cache) -> Option<VecDeque<Message>> {
        let (channel_id, guild_id) = (self.channel.id(), self.channel.guild_id());

        if let Some(mut guild) = cache.guilds.get_mut(&guild_id) {
            guild.viewable_channels.remove(&channel_id);
        }

        // Remove the cached messages for the channel.
        cache.messages.remove(&channel_id.widen()).map(|(_, messages)| messages)
    }
}

impl CacheUpdate for ChannelUpdateEvent {
    type Output = MaybeObfuscated;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        let mut guild = cache.guilds.get_mut(&self.channel.guild_id())?;
        let updated_channel = &self.channel;

        match updated_channel {
            MaybeObfuscated::Viewable(guild_channel) => {
                guild.viewable_channels.insert(guild_channel.clone()).map(Into::into)
            },
            MaybeObfuscated::Obfuscated(_) => {
                guild.viewable_channels.remove(&updated_channel.id()).map(Into::into)
            },
        }
    }
}

impl CacheUpdate for GuildCreateEvent {
    type Output = Vec<GuildId>;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        cache.unavailable_guilds.remove(&self.guild.id);
        let guild = self.guild.clone();

        cache.guilds.insert(self.guild.id, guild);

        if cache.unavailable_guilds.len() == 0 {
            cache.unavailable_guilds.shrink_to_fit();
            Some(cache.guilds.iter().map(|i| *i.key()).collect())
        } else {
            None
        }
    }
}

impl CacheUpdate for GuildDeleteEvent {
    type Output = Guild;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        if self.guild.unavailable {
            cache.unavailable_guilds.insert(self.guild.id, ());
            cache.guilds.remove(&self.guild.id);

            return None;
        }

        match cache.guilds.remove(&self.guild.id) {
            Some(guild) => {
                for channel in &guild.1.viewable_channels {
                    // Remove the channel's cached messages.
                    cache.messages.remove(&channel.id.widen());
                }

                Some(guild.1)
            },
            None => None,
        }
    }
}

impl CacheUpdate for GuildMemberAddEvent {
    type Output = std::convert::Infallible;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        if let Some(mut guild) = cache.guilds.get_mut(&self.member.guild_id) {
            guild.member_count = MemberCount::new(guild.member_count.get() + 1)
                .expect("member count should not overflow");
            guild.members.insert(self.member.clone());
        }

        None
    }
}

impl CacheUpdate for GuildMemberRemoveEvent {
    type Output = Member;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        if let Some(mut guild) = cache.guilds.get_mut(&self.guild_id) {
            guild.member_count = MemberCount::new(guild.member_count.get() - 1)
                .expect("member count should not underflow");
            return guild.members.remove(&self.user.id);
        }

        None
    }
}

impl CacheUpdate for GuildMemberUpdateEvent {
    type Output = Member;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        let mut guild = cache.guilds.get_mut(&self.guild_id)?;
        let old_member = guild.members.get(&self.user.id).cloned();
        let mut new_member = Member {
            __generated_flags: MemberGeneratedFlags::empty(),
            guild_id: self.guild_id,
            nick: self.nick.clone(),
            roles: self.roles.clone(),
            user: self.user.clone(),
            permissions: None,
            avatar: self.avatar,
            communication_disabled_until: self.communication_disabled_until,
        };
        let deaf = self.deaf().unwrap_or_else(|| old_member.as_ref().is_some_and(Member::deaf));
        let mute = self.mute().unwrap_or_else(|| old_member.as_ref().is_some_and(Member::mute));
        new_member.set_deaf(deaf);
        new_member.set_mute(mute);

        if self.user.id == cache.current_user().id
            && let Some(old_member) = &old_member
            && old_member.roles.iter().any(|role| !new_member.roles.contains(role))
            && let Some(channels) = no_longer_viewable_channels(&guild, &new_member)
        {
            obfuscate_no_longer_viewable_channels(&mut guild, channels);
        }

        guild.members.insert(new_member);
        old_member
    }
}

fn no_longer_viewable_channels(guild: &Guild, member: &Member) -> Option<Vec<ChannelId>> {
    let no_longer_viewable: Vec<_> = guild
        .viewable_channels
        .iter()
        .filter(|&channel| !guild.user_permissions_in(channel, member).view_channel())
        .map(|channel| channel.id)
        .collect();
    (!no_longer_viewable.is_empty()).then_some(no_longer_viewable)
}

fn obfuscate_no_longer_viewable_channels(
    guild: &mut dashmap::mapref::one::RefMut<'_, GuildId, Guild>,
    channels: Vec<ChannelId>,
) {
    for id in channels {
        guild.viewable_channels.remove(&id);
    }
}

impl CacheUpdate for GuildMembersChunkEvent {
    type Output = std::convert::Infallible;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        if let Some(mut g) = cache.guilds.get_mut(&self.guild_id) {
            g.members.extend(self.members.clone());
        }

        None
    }
}

impl CacheUpdate for GuildRoleCreateEvent {
    type Output = std::convert::Infallible;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        cache.guilds.get_mut(&self.role.guild_id).map(|mut g| g.roles.insert(self.role.clone()));
        None
    }
}

impl CacheUpdate for GuildRoleDeleteEvent {
    type Output = Role;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        cache.guilds.get_mut(&self.guild_id).and_then(|mut g| g.roles.remove(&self.role_id))
    }
}

impl CacheUpdate for GuildRoleUpdateEvent {
    type Output = Role;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        let mut guild = cache.guilds.get_mut(&self.role.guild_id)?;
        let old_role = guild
            .roles
            .get_mut(&self.role.id)
            .map(|mut role| std::mem::replace(&mut *role, self.role.clone()));

        if let Some(old_role) = &old_role
            && let Some(member) = guild.members.get(&cache.current_user().id)
            && member.roles.contains(&self.role.id)
            // Check for permissions that affect channel visibility to avoid rescanning
            // channels for non-consequential role updates.
            && (old_role.permissions.view_channel() && !self.role.permissions.view_channel()
                // Also check administrator since it can be toggled separately from view channel.
                || old_role.permissions.administrator() && !self.role.permissions.administrator())
            && let Some(channels) = no_longer_viewable_channels(&guild, member)
        {
            obfuscate_no_longer_viewable_channels(&mut guild, channels);
        }

        old_role
    }
}

impl CacheUpdate for GuildUpdateEvent {
    type Output = std::convert::Infallible;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        if let Some(mut guild) = cache.guilds.get_mut(&self.guild.id) {
            guild.icon.clone_from(&self.guild.icon);
            guild.name.clone_from(&self.guild.name);
            guild.owner_id.clone_from(&self.guild.owner_id);
            guild.roles.clone_from(&self.guild.roles);
        }

        None
    }
}

impl CacheUpdate for MessageCreateEvent {
    /// The oldest message, if the channel's message cache was already full.
    type Output = Message;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        // Add the new message to the cache and remove the oldest cached message.
        let max = cache.settings().max_messages;

        if max == 0 {
            return None;
        }

        let mut messages = cache.messages.entry(self.message.channel_id).or_default();

        let mut removed_msg = None;
        if messages.len() == max {
            removed_msg = messages.pop_front();
        }

        if !messages.iter().any(|m| m.id == self.message.id) {
            messages.push_back(self.message.clone());
        }

        removed_msg
    }
}

impl CacheUpdate for MessageUpdateEvent {
    type Output = Message;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        for message in cache.messages.get_mut(&self.message.channel_id)?.iter_mut() {
            if message.id == self.message.id {
                let old_message = message.clone();
                message.clone_from(&self.message);
                return Some(old_message);
            }
        }

        None
    }
}

impl CacheUpdate for ReadyEvent {
    type Output = NonZeroU16;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        for unavailable in &self.ready.guilds {
            cache.guilds.remove(&unavailable.id);
            cache.unavailable_guilds.insert(unavailable.id, ());
        }

        let shard_info = self.ready.shard.unwrap_or_default();

        cache.user.write().clone_from(&self.ready.user);

        let mut shards = cache.shard_data.write();
        shards.total = shard_info.total;
        shards.connected.insert(shard_info.id);

        if shards.connected.len() == shards.total.get() as usize && !shards.has_sent_shards_ready {
            shards.has_sent_shards_ready = true;
            Some(shards.total)
        } else {
            None
        }
    }
}

impl CacheUpdate for ThreadCreateEvent {
    type Output = GuildThread;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        cache
            .guilds
            .get_mut(&self.thread.base.guild_id)
            .and_then(|mut g| g.threads.insert(self.thread.clone()))
    }
}

impl CacheUpdate for ThreadUpdateEvent {
    type Output = GuildThread;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        cache
            .guilds
            .get_mut(&self.thread.base.guild_id)
            .and_then(|mut g| g.threads.insert(self.thread.clone()))
    }
}

impl CacheUpdate for ThreadDeleteEvent {
    type Output = GuildThread;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        cache
            .guilds
            .get_mut(&self.thread.guild_id)
            .and_then(|mut g| g.threads.remove(&self.thread.id))
    }
}

impl CacheUpdate for ThreadListSyncEvent {
    type Output = std::convert::Infallible;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        let mut guild = cache.guilds.get_mut(&self.guild_id)?;
        let Some(channel_ids) = &self.channel_ids else {
            // channel_ids is none, this is a full sync, easy path
            guild.threads.clone_from(&self.threads);
            return None;
        };

        // Add new threads and update existing threads.
        for new_thread in &self.threads {
            match guild.threads.entry(&new_thread.id) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().clone_from(new_thread);
                },
                Entry::Vacant(entry) => {
                    entry.insert(new_thread.clone());
                },
            }
        }

        // Remove threads which are not provided in the sync and are in the provided channels.
        let mut removed_threads = Vec::new();
        for &channel_id in channel_ids {
            for thread in &guild.threads {
                if thread.parent_id != channel_id {
                    continue;
                }

                if !self.threads.contains_key(&thread.id) {
                    removed_threads.push(thread.id);
                }
            }
        }

        for to_remove in removed_threads {
            guild.threads.remove(&to_remove);
        }

        None
    }
}

impl CacheUpdate for UserUpdateEvent {
    type Output = CurrentUser;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        let mut user = cache.user.write();
        Some(std::mem::replace(&mut user, self.current_user.clone()))
    }
}

impl CacheUpdate for VoiceStateUpdateEvent {
    type Output = VoiceState;

    fn update(&self, cache: &Cache) -> Option<VoiceState> {
        let guild_id = self.voice_state.guild_id?;
        let mut guild = cache.guilds.get_mut(&guild_id)?;

        if let Some(member) = &self.voice_state.member {
            guild.members.insert(member.clone());
        }

        let old_state = guild.voice_states.remove(&self.voice_state.user_id);
        if self.voice_state.channel_id.is_some() {
            guild.voice_states.insert(self.voice_state.clone());
        }

        // Obfuscate the voice channel if the bot never had view channel access, e.g.,
        // the bot was moved into the channel by someone with Permissions::MOVE_MEMBERS.
        if self.voice_state.user_id == cache.current_user().id
            && let Some(old_state) = &old_state
            && let Some(channel_id) = &old_state.channel_id
            && let Some(channel) = guild.viewable_channels.get(channel_id)
            && let Some(member) = guild.members.get(&self.voice_state.user_id)
            && !guild.user_permissions_in(channel, member).view_channel()
        {
            guild.viewable_channels.remove(channel_id);
        }

        old_state
    }
}

impl CacheUpdate for ReactionAddEvent {
    type Output = Message;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        let reaction = &self.reaction;
        let mut messages = cache.messages.get_mut(&reaction.channel_id)?;

        for message in messages.iter_mut() {
            if message.id != reaction.message_id {
                continue;
            }

            let prev = message.clone();

            if let Some(existing) =
                message.reactions.iter_mut().find(|r| r.reaction_type == reaction.emoji)
            {
                existing.count += 1;
                if reaction.burst {
                    existing.count_details.burst += 1;
                } else {
                    existing.count_details.normal += 1;
                }
                return Some(prev);
            }

            let me = self.reaction.user_id == Some(cache.current_user().id);
            let new_reaction = MessageReaction {
                me,
                burst_colours: reaction.burst_colours.clone().unwrap_or_default(),
                count: 1,
                count_details: CountDetails {
                    burst: u64::from(reaction.burst),
                    normal: u64::from(!reaction.burst),
                },
                me_burst: me && reaction.burst,
                reaction_type: reaction.emoji.clone(),
            };

            message.reactions.push(new_reaction);

            return Some(prev);
        }

        None
    }
}

impl CacheUpdate for ReactionRemoveEvent {
    type Output = Message;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        let reaction = &self.reaction;
        let mut messages = cache.messages.get_mut(&reaction.channel_id)?;

        for message in messages.iter_mut() {
            if message.id != reaction.message_id {
                continue;
            }

            let old_message = message.clone();

            let index = message.reactions.iter().position(|r| r.reaction_type == reaction.emoji)?;

            let existing_reaction = &mut message.reactions[index];

            existing_reaction.count -= 1;
            if reaction.burst {
                existing_reaction.count_details.burst -= 1;
            } else {
                existing_reaction.count_details.normal -= 1;
            }

            if existing_reaction.count == 0 {
                message.reactions.remove(index);
            }

            return Some(old_message);
        }

        None
    }
}

impl CacheUpdate for ReactionRemoveEmojiEvent {
    type Output = Message;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        let reaction = &self.reaction;
        let mut messages = cache.messages.get_mut(&reaction.channel_id)?;

        for message in messages.iter_mut() {
            if message.id != reaction.message_id {
                continue;
            }

            let old_message = message.clone();

            let index = message.reactions.iter().position(|r| r.reaction_type == reaction.emoji)?;

            message.reactions.remove(index);

            return Some(old_message);
        }

        None
    }
}

impl CacheUpdate for ReactionRemoveAllEvent {
    type Output = Message;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        let mut messages = cache.messages.get_mut(&self.channel_id)?;

        for message in messages.iter_mut() {
            if message.id != self.message_id {
                continue;
            }

            let old_message = message.clone();

            message.reactions.clear();

            return Some(old_message);
        }

        None
    }
}
