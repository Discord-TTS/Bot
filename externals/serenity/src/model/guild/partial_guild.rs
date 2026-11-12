use serde::Serialize;

#[cfg(feature = "model")]
use crate::builder::EditGuild;
#[cfg(feature = "model")]
use crate::http::Http;
use crate::model::prelude::*;
#[cfg(feature = "model")]
use crate::model::utils::icon_url;

/// Partial information about a [`Guild`]. This does not include information like member data.
///
/// [Discord docs](https://docs.discord.com/developers/resources/guild#guild-object).
#[bool_to_bitflags::bool_to_bitflags]
#[cfg_attr(feature = "typesize", derive(typesize::derive::TypeSize))]
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(remote = "Self")]
#[non_exhaustive]
pub struct PartialGuild {
    // ======
    // These fields are copy-pasted from the top part of Guild, and the omitted fields filled in
    // ======
    /// The unique Id identifying the guild.
    ///
    /// This is equivalent to the Id of the default role (`@everyone`).
    pub id: GuildId,
    /// The name of the guild.
    pub name: FixedString,
    /// The hash of the icon used by the guild.
    ///
    /// In the client, this appears on the guild list on the left-hand side.
    pub icon: Option<ImageHash>,
    // Omitted `owner` field because only Http::get_guilds uses it, which returns GuildInfo
    /// The Id of the [`User`] who owns the guild.
    pub owner_id: UserId,
    /// A mapping of the guild's roles.
    pub roles: ExtractMap<RoleId, Role>,
}

#[cfg(feature = "model")]
impl PartialGuild {
    /// Edits the current guild with new data where specified.
    ///
    /// **Note**: Requires the [Manage Guild] permission.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] if the current user lacks permission or if invalid data is given.
    ///
    /// [Manage Guild]: Permissions::MANAGE_GUILD
    pub async fn edit(&mut self, http: &Http, builder: EditGuild<'_>) -> Result<()> {
        *self = self.id.edit(http, builder).await?;
        Ok(())
    }

    /// Gets a partial amount of guild data by its Id.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] if the current user is not
    /// in the guild.
    pub async fn get(cache_http: impl CacheHttp, guild_id: GuildId) -> Result<PartialGuild> {
        guild_id.to_partial_guild(cache_http).await
    }

    /// Calculate a [`Member`]'s permissions in the guild.
    ///
    /// You likely want to use PartialGuild::user_permissions_in instead as this function does not
    /// consider permission overwrites.
    #[must_use]
    pub fn member_permissions(&self, member: &Member) -> Permissions {
        Guild::user_permissions_in_(
            None,
            member.user.id,
            &member.roles,
            self.id,
            &self.roles,
            self.owner_id,
        )
    }

    /// Calculate a [`PartialMember`]'s permissions in the guild.
    ///
    /// You likely want to use PartialGuild::partial_member_permissions_in instead as this function
    /// does not consider permission overwrites.
    ///
    /// # Panics
    ///
    /// Panics if the passed [`UserId`] does not match the [`PartialMember`] id, if user is Some.
    #[must_use]
    pub fn partial_member_permissions(
        &self,
        member_id: UserId,
        member: &PartialMember,
    ) -> Permissions {
        if let Some(user) = &member.user {
            assert_eq!(user.id, member_id, "User::id does not match provided PartialMember");
        }

        Guild::user_permissions_in_(
            None,
            member_id,
            &member.roles,
            self.id,
            &self.roles,
            self.owner_id,
        )
    }

    /// Gets the highest role a [`Member`] of this Guild has.
    ///
    /// Returns None if the member has no roles or the member from this guild.
    #[must_use]
    pub fn member_highest_role(&self, member: &Member) -> Option<&Role> {
        Guild::member_highest_role_in_(&self.roles, member)
    }

    /// See [`Guild::greater_member_hierarchy`] for more information.
    ///
    /// Note that unlike [`Guild::greater_member_hierarchy`], this method requires a [`Member`] as
    /// member data is not available on a [`PartialGuild`].
    #[must_use]
    pub fn greater_member_hierarchy(&self, lhs: &Member, rhs: &Member) -> Option<UserId> {
        let lhs_highest_role = self.member_highest_role(lhs);
        let rhs_highest_role = self.member_highest_role(rhs);

        Guild::greater_member_hierarchy_in_(
            lhs_highest_role,
            rhs_highest_role,
            self.owner_id,
            lhs,
            rhs,
        )
    }

    /// Calculate a [`PartialMember`]'s permissions in a given channel in a guild.
    ///
    /// **Note**: Bots cannot view permission overwrites on obfuscated channels. Results may be
    /// inaccurate if the channel is an obfuscated channel with permission overwrites relevant to
    /// the member.
    ///
    /// # Panics
    ///
    /// Panics if the passed [`UserId`] does not match the [`PartialMember`] id, if user is Some.
    #[must_use]
    pub fn partial_member_permissions_in<'a>(
        &self,
        channel: impl Into<GuildChannelRef<'a>>,
        member_id: UserId,
        member: &PartialMember,
    ) -> Permissions {
        if let Some(user) = &member.user {
            assert_eq!(user.id, member_id, "User::id does not match provided PartialMember");
        }

        Guild::user_permissions_in_(
            Some(channel.into()),
            member_id,
            &member.roles,
            self.id,
            &self.roles,
            self.owner_id,
        )
    }

    /// Returns a formatted URL of the guild's icon, if the guild has an icon.
    #[must_use]
    pub fn icon_url(&self) -> Option<String> {
        icon_url(self.id, self.icon.as_ref())
    }

    /// Calculate a [`Member`]'s permissions in a given channel in the guild.
    ///
    /// **Note**: Bots cannot view permission overwrites on obfuscated channels. Results may be
    /// inaccurate if the channel is an obfuscated channel with permission overwrites relevant to
    /// the member.
    #[must_use]
    pub fn user_permissions_in<'a>(
        &self,
        channel: impl Into<GuildChannelRef<'a>>,
        member: &Member,
    ) -> Permissions {
        Guild::user_permissions_in_(
            Some(channel.into()),
            member.user.id,
            &member.roles,
            self.id,
            &self.roles,
            self.owner_id,
        )
    }

    /// Obtain a reference to a role by its name.
    ///
    /// **Note**: If two or more roles have the same name, obtained reference will be one of them.
    ///
    /// # Examples
    ///
    /// Obtain a reference to a [`Role`] by its name.
    ///
    /// ```rust,no_run
    /// # use serenity::model::prelude::*;
    /// # use serenity::prelude::*;
    ///
    /// # #[cfg(feature = "cache")]
    /// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// # let cache: serenity::cache::Cache = unimplemented!();
    /// # let msg: Message = unimplemented!();
    ///
    /// if let Some(guild_id) = msg.guild_id {
    ///     if let Some(guild) = guild_id.to_guild_cached(&cache) {
    ///         if let Some(role) = guild.role_by_name("role_name") {
    ///             println!("Obtained role's reference: {:?}", role);
    ///         }
    ///     }
    /// }
    ///
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn role_by_name(&self, role_name: &str) -> Option<&Role> {
        self.roles.iter().find(|role| role_name == &*role.name)
    }
}

// Manual impl needed to insert guild_id into Role's
impl<'de> Deserialize<'de> for PartialGuildGeneratedOriginal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> StdResult<Self, D::Error> {
        let mut guild = Self::deserialize(deserializer)?; // calls #[serde(remote)]-generated inherent method
        guild.roles.iter_mut().for_each(|r| r.guild_id = guild.id);
        Ok(guild)
    }
}

impl Serialize for PartialGuildGeneratedOriginal {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> StdResult<S::Ok, S::Error> {
        Self::serialize(self, serializer) // calls #[serde(remote)]-generated inherent method
    }
}

impl From<Guild> for PartialGuild {
    /// Converts this [`Guild`] instance into a [`PartialGuild`]
    fn from(guild: Guild) -> Self {
        Self {
            __generated_flags: PartialGuildGeneratedFlags::empty(),
            id: guild.id,
            icon: guild.icon,
            name: guild.name,
            owner_id: guild.owner_id,
            roles: guild.roles,
        }
    }
}
