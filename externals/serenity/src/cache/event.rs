use std::collections::VecDeque;
use std::num::NonZeroU16;

use extract_map::entry::Entry;

use super::{Cache, CacheUpdate};
use crate::model::prelude::*;

impl CacheUpdate for ChannelCreateEvent {
    type Output = GuildChannel;

    fn update(&self, cache: &Cache) -> Option<Self::Output> {
        cache
            .guilds
            .get_mut(&self.channel.base.guild_id)
            .and_then(|mut g| g.channels.insert(self.channel.clone()))
    }
}

impl CacheUpdate for ChannelDeleteEvent {
    type Output = VecDeque<Message>;

    fn update(&self, cache: &Cache) -> Option<VecDeque<Message>> {
        let (channel_id, guild_id) = (self.channel.id, self.channel.base.guild_id);

        cache.guilds.get_mut(&guild_id).map(|mut g| g.channels.remove(&channel_id));

        // Remove the cached messages for the channel.
        cache.messages.remove(&channel_id.widen()).map(|(_, messages)| messages)
    }
}

impl CacheUpdate for ChannelUpdateEvent {
    type Output = GuildChannel;

    fn update(&self, cache: &Cache) -> Option<GuildChannel> {
        cache
            .guilds
            .get_mut(&self.channel.base.guild_id)
            .and_then(|mut g| g.channels.insert(self.channel.clone()))
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
                for channel in &guild.1.channels {
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
        if let Some(mut guild) = cache.guilds.get_mut(&self.guild_id) {
            let item = if let Some(mut member) = guild.members.get_mut(&self.user.id) {
                let item = Some(member.clone());

                member.nick.clone_from(&self.nick);
                member.roles.clone_from(&self.roles);
                member.user.clone_from(&self.user);
                member.avatar.clone_from(&self.avatar);
                member.communication_disabled_until.clone_from(&self.communication_disabled_until);
                member.set_deaf(self.deaf());
                member.set_mute(self.mute());

                item
            } else {
                None
            };

            if item.is_none() {
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

                new_member.set_deaf(self.deaf());
                new_member.set_mute(self.mute());

                guild.members.insert(new_member);
            }

            item
        } else {
            None
        }
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
        if let Some(mut guild) = cache.guilds.get_mut(&self.role.guild_id)
            && let Some(mut role) = guild.roles.get_mut(&self.role.id)
        {
            return Some(std::mem::replace(&mut *role, self.role.clone()));
        }

        None
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
        if let Some(guild_id) = self.voice_state.guild_id {
            if let Some(mut guild) = cache.guilds.get_mut(&guild_id) {
                if let Some(member) = &self.voice_state.member {
                    guild.members.insert(member.clone());
                }

                if self.voice_state.channel_id.is_some() {
                    // Update or add to the voice state list
                    let old_state = guild.voice_states.remove(&self.voice_state.user_id);
                    guild.voice_states.insert(self.voice_state.clone());
                    old_state
                } else {
                    // Remove the user from the voice state list
                    guild.voice_states.remove(&self.voice_state.user_id)
                }
            } else {
                None
            }
        } else {
            None
        }
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
