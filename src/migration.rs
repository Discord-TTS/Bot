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

use std::{sync::Arc, io::Write};

use crate::structs::{Error, TTSMode};
use crate::constants::DB_SETUP_QUERY;

async fn migrate_single_to_modes(transaction: &mut deadpool_postgres::Transaction<'_>, table: &str, new_table: &str, old_column: &str, id_column: &str) -> Result<(), Error> {
    let insert_query_mode = transaction.prepare_cached(&format!("INSERT INTO {new_table}({id_column}, mode, voice) VALUES ($1, $2, $3)")).await?;
    let insert_query_voice = transaction.prepare_cached(&format!("
        INSERT INTO {table}({id_column}, voice_mode) VALUES ($1, $2)
        ON CONFLICT ({id_column}) DO UPDATE SET voice_mode = EXCLUDED.voice_mode
    ")).await?;

    let mut delete_voice = false;
    for row in transaction.query(&format!("SELECT * FROM {table}"), &[]).await? {
        let voice: Result<Option<String>, _> = row.try_get(old_column);
        if let Ok(voice) = voice {
            delete_voice = true;
            if let Some(voice) = voice {
                let column_id: i64 = row.get(id_column);

                transaction.execute(&insert_query_voice, &[&column_id, &TTSMode::Gtts]).await?;
                transaction.execute(&insert_query_mode, &[&column_id, &TTSMode::Gtts, &voice]).await?;
            }
        } else {
            break
        }
    };

    if delete_voice {
        transaction.execute(&format!("ALTER TABLE {table} DROP COLUMN {old_column}"), &[]).await?;
    };

    Ok(())
}

// I'll use a proper framework for this one day
pub async fn run(config: &mut toml::Value, pool: &Arc<deadpool_postgres::Pool>) -> Result<(), Error> {
    let starting_conf = config.clone();

    let mut conn = pool.get().await?;
    let mut transaction = conn.transaction().await?;
    let main_config = config["Main"].as_table_mut().unwrap();

    if main_config.get("setup").is_none() {
        transaction.batch_execute(DB_SETUP_QUERY).await?;
        main_config.insert(String::from("setup"), toml::Value::Boolean(true));
    }

    transaction.batch_execute("
        DO $$ BEGIN
            CREATE type TTSMode AS ENUM (
                'gtts',
                'espeak',
                'premium'
            );
        EXCEPTION
            WHEN duplicate_object THEN null;
        END $$;

        CREATE TABLE IF NOT EXISTS guild_voice (
            guild_id      bigint,
            mode          TTSMode,
            voice         text     NOT NULL,

            PRIMARY KEY (guild_id, mode),

            FOREIGN KEY       (guild_id)
            REFERENCES guilds (guild_id)
            ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS user_voice (
            user_id       bigint,
            mode          TTSMode,
            voice         text,

            PRIMARY KEY (user_id, mode),

            FOREIGN KEY         (user_id)
            REFERENCES userinfo (user_id)
            ON DELETE CASCADE
        );

        ALTER TABLE userinfo
            ADD COLUMN IF NOT EXISTS voice_mode     TTSMode,
            ADD COLUMN IF NOT EXISTS speaking_rate  real       DEFAULT 1;
        ALTER TABLE guilds
            ADD COLUMN IF NOT EXISTS audience_ignore  bool       DEFAULT True,
            ADD COLUMN IF NOT EXISTS voice_mode       TTSMode    DEFAULT 'gtts',
            ADD COLUMN IF NOT EXISTS to_translate     bool       DEFAULT False,
            ADD COLUMN IF NOT EXISTS target_lang      varchar(5),
            ADD COLUMN IF NOT EXISTS premium_user     bigint,
            ADD COLUMN IF NOT EXISTS require_voice    bool       DEFAULT True;

        INSERT INTO user_voice  (user_id, mode)         VALUES(0, 'gtts')       ON CONFLICT (user_id, mode)  DO NOTHING;
        INSERT INTO guild_voice (guild_id, mode, voice) VALUES(0, 'gtts', 'en') ON CONFLICT (guild_id, mode) DO NOTHING;
    ").await?;

    migrate_single_to_modes(&mut transaction, "userinfo", "user_voice", "voice", "user_id").await?;
    migrate_single_to_modes(&mut transaction, "guilds", "guild_voice", "default_voice", "guild_id").await?;

    if &starting_conf != config {
        let mut config_file = std::fs::File::create("config.toml")?;
        config_file.write_all(toml::to_string_pretty(config)?.as_bytes())?;
    }

    transaction.commit().await?;
    Ok(())
}
