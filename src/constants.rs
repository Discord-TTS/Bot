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

pub const RED: u32 = 0xff0000;
pub const NETURAL_COLOUR: u32 = 
    if cfg!(feature="premium") {
        0xcaa652
    } else {
        0x3498db
    };

#[cfg(feature="premium")]
pub const TRANSLATION_URL: &str = "https://api-free.deepl.com/v2";

pub const OPTION_SEPERATORS: [&str; 4] = [
    ":small_orange_diamond:",
    ":small_blue_diamond:",
    ":small_red_triangle:",
    ":star:"
];

pub const DM_WELCOME_MESSAGE: &str = "
**All messages after this will be sent to a private channel where we can assist you.**
Please keep in mind that we aren't always online and get a lot of messages, so if you don't get a response within a day repeat your message.
There are some basic rules if you want to get help though:
`1.` Ask your question, don't just ask for help
`2.` Don't spam, troll, or send random stuff (including server invites)
`3.` Many questions are answered in `-help`, try that first (also the default prefix is `-`)
";

pub fn db_setup_query() -> String {
    format!("{}{}{}",
        if cfg!(feature="premium") {"
            CREATE TABLE userinfo (
                user_id       bigint     PRIMARY KEY,
                dm_blocked    bool       DEFAULT False,
                dm_welcomed   bool       DEFAULT false,
                speaking_rate real       DEFAULT 1,
                voice         text
            );"} else {"
            CREATE TABLE userinfo (
                user_id      bigint     PRIMARY KEY,
                dm_blocked   bool       DEFAULT False,
                dm_welcomed  bool       DEFAULT false,
                voice        text
            );"}, if cfg!(feature="premium") {"
            CREATE TABLE guilds (
                guild_id        bigint     PRIMARY KEY,
                channel         bigint     DEFAULT 0,
                premium_user    bigint,
                xsaid           bool       DEFAULT True,
                bot_ignore      bool       DEFAULT True,
                auto_join       bool       DEFAULT False,
                to_translate    bool       DEFAULT False,
                formal          bool,
                msg_length      smallint   DEFAULT 30,
                repeated_chars  smallint   DEFAULT 0,
                prefix          varchar(6) DEFAULT 'p-',
                target_lang     varchar(5),
                default_voice   text,
                audience_ignore bool       DEFAULT True,

                FOREIGN KEY         (premium_user)
                REFERENCES userinfo (user_id)
                ON DELETE CASCADE
            );"
        } else {"
            CREATE TABLE guilds (
                guild_id        bigint     PRIMARY KEY,
                channel         bigint     DEFAULT 0,
                xsaid           bool       DEFAULT True,
                bot_ignore      bool       DEFAULT True,
                auto_join       bool       DEFAULT False,
                msg_length      smallint   DEFAULT 30,
                repeated_chars  smallint   DEFAULT 0,
                prefix          varchar(6) DEFAULT '-',
                default_voice   text,
                audience_ignore bool       DEFAULT True
            );
        "}, "
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
    ")
}
