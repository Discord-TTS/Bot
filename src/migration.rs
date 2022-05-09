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

use std::io::Write;

use sqlx::{Row, Executor, Connection as _};

use crate::structs::{Result, TTSMode, OptionTryUnwrap};
use crate::constants::DB_SETUP_QUERY;

type Transaction<'a> = sqlx::Transaction<'a, sqlx::Postgres>;

async fn migrate_single_to_modes(transaction: &mut Transaction<'_>, table: &str, new_table: &str, old_column: &str, id_column: &str) -> Result<()> {
    let insert_query_mode = format!("INSERT INTO {new_table}({id_column}, mode, voice) VALUES ($1, $2, $3)");
    let insert_query_voice = format!("
        INSERT INTO {table}({id_column}, voice_mode) VALUES ($1, $2)
        ON CONFLICT ({id_column}) DO UPDATE SET voice_mode = EXCLUDED.voice_mode
    ");

    let mut delete_voice = false;
    for row in transaction.fetch_all(&*format!("SELECT * FROM {table}")).await? { 
        if let Ok(voice) = row.try_get::<Option<String>, _>(old_column) {
            delete_voice = true;
            if let Some(voice) = voice {
                let column_id: i64 = row.get(id_column);

                transaction.execute(sqlx::query(&insert_query_voice).bind(column_id).bind(TTSMode::gTTS)).await?;
                transaction.execute(sqlx::query(&insert_query_mode).bind(column_id).bind(TTSMode::gTTS).bind(voice)).await?;
            }
        } else {
            break
        }
    };

    if delete_voice {
        transaction.execute(&*format!("ALTER TABLE {table} DROP COLUMN {old_column}")).await?;
    };

    Ok(())
}

async fn migrate_speaking_rate_to_mode(transaction: &mut Transaction<'_>) -> Result<()> {
    let insert_query = "
        INSERT INTO user_voice(user_id, mode, speaking_rate) VALUES ($1, $2, $3)
        ON CONFLICT (user_id, mode) DO UPDATE SET speaking_rate = EXCLUDED.speaking_rate
    ";

    let mut delete_column = false;
    for row in transaction.fetch_all("SELECT * FROM userinfo").await? {
        if let Ok(speaking_rate) = row.try_get::<f32, _>("speaking_rate") {
            delete_column = true;

            if (speaking_rate - 1.0).abs() > f32::EPSILON {
                let user_id: i64 = row.get("user_id");
                transaction.execute(sqlx::query(insert_query).bind(user_id).bind(TTSMode::gCloud).bind(speaking_rate)).await?;
            }
        } else {
            break
        }
    };

    if delete_column {
        transaction.execute("ALTER TABLE userinfo DROP COLUMN speaking_rate").await?;
    };

    Ok(())
}

// I'll use a proper framework for this one day
pub async fn run(config: &mut toml::Value, pool: &sqlx::PgPool) -> Result<()> {
    let starting_conf = config.clone();    
    let mut config_clone = config.clone();

    *config = pool.acquire().await?.transaction::<_, _, anyhow::Error>(move |transaction| Box::pin(async move {
        let main_config = config_clone["Main"].as_table_mut().try_unwrap()?;
        _run(main_config, transaction).await?;
        Ok(config_clone)
    })).await?;

    if &starting_conf != config {
        let mut config_file = std::fs::File::create("config.toml")?;
        config_file.write_all(toml::to_string_pretty(&config)?.as_bytes())?;
    };

    Ok(())
}

async fn _run(main_config: &mut toml::value::Table, transaction: &mut Transaction<'_>) -> Result<()> {
    if main_config.get("setup").is_none() {
        transaction.execute(DB_SETUP_QUERY).await?;
        main_config.insert(String::from("setup"), toml::Value::Boolean(true));
    }

    transaction.execute("
        DO $$ BEGIN
            CREATE type TTSMode AS ENUM (
                'gtts',
                'espeak',
                'premium'
            );

            ALTER TYPE TTSMode RENAME VALUE 'premium' TO 'gcloud';
            ALTER TYPE TTSMode ADD VALUE 'polly';
        EXCEPTION
            WHEN OTHERS THEN null;
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
            ADD COLUMN IF NOT EXISTS voice_mode            TTSMode,
            ADD COLUMN IF NOT EXISTS premium_voice_mode    TTSMode;
        ALTER TABLE guilds
            ADD COLUMN IF NOT EXISTS audience_ignore  bool       DEFAULT True,
            ADD COLUMN IF NOT EXISTS voice_mode       TTSMode    DEFAULT 'gtts',
            ADD COLUMN IF NOT EXISTS to_translate     bool       DEFAULT False,
            ADD COLUMN IF NOT EXISTS target_lang      varchar(5),
            ADD COLUMN IF NOT EXISTS premium_user     bigint,
            ADD COLUMN IF NOT EXISTS require_voice    bool       DEFAULT True;
        ALTER TABLE user_voice
            ADD COLUMN IF NOT EXISTS speaking_rate real;

        -- The old table had a pkey on traceback, now we hash and pkey on that
        ALTER TABLE errors
            ADD COLUMN IF NOT EXISTS traceback_hash bytea;
        DELETE FROM errors WHERE traceback_hash IS NULL;
        ALTER TABLE errors
            DROP CONSTRAINT IF EXISTS errors_pkey,
            DROP CONSTRAINT IF EXISTS traceback_hash_pkey,
            ADD CONSTRAINT traceback_hash_pkey PRIMARY KEY (traceback_hash);

        INSERT INTO user_voice  (user_id, mode)         VALUES(0, 'gtts')       ON CONFLICT (user_id, mode)  DO NOTHING;
        INSERT INTO guild_voice (guild_id, mode, voice) VALUES(0, 'gtts', 'en') ON CONFLICT (guild_id, mode) DO NOTHING;
    ").await?;

    migrate_single_to_modes(transaction, "userinfo", "user_voice", "voice", "user_id").await?;
    migrate_single_to_modes(transaction, "guilds", "guild_voice", "default_voice", "guild_id").await?;
    migrate_speaking_rate_to_mode(transaction).await?;
    Ok(())
}
