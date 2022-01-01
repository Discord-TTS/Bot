// Discord TTS Bot
// Copyright (C) 2021-Present David Thomas

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::{collections::HashMap, sync::Arc};

use lavalink_rs::LavalinkClient;
use poise::serenity_prelude as serenity;

use crate::{database::DatabaseHandler, analytics::AnalyticsHandler};

pub struct Config {
    pub server_invite: String,
    pub invite_channel: u64,
    pub main_server: u64,
    pub ofs_role: u64,
}

pub struct Data {
    pub analytics: Arc<AnalyticsHandler>,
    pub guilds_db: DatabaseHandler<u64>,
    pub userinfo_db: DatabaseHandler<u64>,
    pub nickname_db: DatabaseHandler<[u64; 2]>,

    pub webhooks: HashMap<String, serenity::Webhook>,
    pub start_time: std::time::SystemTime,
    pub owner_id: serenity::UserId,
    pub lavalink: LavalinkClient,
    pub reqwest: reqwest::Client,
    pub config: Config,
}


#[derive(Debug)]
pub enum Error {
    GuildOnly,
    DebugLog(&'static str), // debug log something but ignore
    Unexpected(Box<dyn std::error::Error + Send + Sync>),
}

impl<E> From<E> for Error
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn from(e: E) -> Self {
        Self::Unexpected(e.into())
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

pub type Context<'a> = poise::Context<'a, Data, Error>;
pub const NETURAL_COLOUR: u32 = 0x3498db;
pub const RED: u32 = 0xff0000;

pub const OPTION_SEPERATORS: [&str; 3] = [
    ":small_orange_diamond:",
    ":small_blue_diamond:",
    ":small_red_triangle:"
];

pub const DM_WELCOME_MESSAGE: &str = "
**All messages after this will be sent to a private channel where we can assist you.**
Please keep in mind that we aren't always online and get a lot of messages, so if you don't get a response within a day repeat your message.
There are some basic rules if you want to get help though:
`1.` Ask your question, don't just ask for help
`2.` Don't spam, troll, or send random stuff (including server invites)
`3.` Many questions are answered in `-help`, try that first (also the default prefix is `-`)
";

pub const DB_SETUP_QUERY: &str = "
    CREATE TABLE guilds (
        guild_id       bigint     PRIMARY KEY,
        channel        bigint     DEFAULT 0,
        xsaid          bool       DEFAULT True,
        bot_ignore     bool       DEFAULT True,
        auto_join      bool       DEFAULT False,
        msg_length     smallint   DEFAULT 30,
        repeated_chars smallint   DEFAULT 0,
        prefix         varchar(6) DEFAULT '-',
        default_lang   varchar(5)
    );

    CREATE TABLE userinfo (
        user_id      bigint     PRIMARY KEY,
        dm_blocked   bool       DEFAULT False,
        dm_welcomed  bool       DEFAULT false,
        lang         varchar(5)
    );

    CREATE TABLE nicknames (
        guild_id bigint,
        user_id  bigint,
        name     text,

        PRIMARY KEY (guild_id, user_id),

        FOREIGN KEY       (guild_id)
        REFERENCES guilds (guild_id)
        ON DELETE CASCADE,

        FOREIGN KEY         (user_id)
        REFERENCES userinfo (user_id)
        ON DELETE CASCADE
    );

    CREATE TABLE analytics (
        event          text  NOT NULL,
        count          int   NOT NULL,
        is_command     bool  NOT NULL,
        date_collected date  NOT NULL DEFAULT CURRENT_DATE,
        PRIMARY KEY (event, is_command, date_collected)
    );

    CREATE TABLE errors (
        traceback   text    PRIMARY KEY,
        message_id  bigint  NOT NULL,
        occurrences int     DEFAULT 1
    );

    INSERT INTO guilds(guild_id) VALUES(0);
    INSERT INTO userinfo(user_id) VALUES(0);
    INSERT INTO NICKNAMES(guild_id, user_id) VALUES (0, 0);
";
