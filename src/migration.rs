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

use crate::structs::Error;
use crate::constants::db_setup_query;

// I'll use a proper framework for this one day
pub async fn run(config: &mut toml::Value, pool: &Arc<deadpool_postgres::Pool>) -> Result<(), Error> {
    let starting_conf = config.clone();

    let mut conn = pool.get().await?;
    let transaction = conn.transaction().await?;
    let main_config = config["Main"].as_table_mut().unwrap();

    if main_config.get("setup").is_none() {
        transaction.batch_execute(&db_setup_query()).await?;
        main_config.insert(String::from("setup"), toml::Value::Boolean(true));
    }

    if cfg!(feature="premium") {
        let default_user_row = transaction.query_one("SELECT * FROM userinfo WHERE user_id = 0", &[]).await?;
        if default_user_row.try_get::<&str, f32>("speaking_rate").is_err() {
            transaction.execute("ALTER TABLE userinfo ADD COLUMN speaking_rate real DEFAULT 1", &[]).await?;
        }
    }

    transaction.execute("ALTER TABLE guilds ADD COLUMN IF NOT EXISTS audience_ignore bool DEFAULT True", &[]).await?;

    if &starting_conf != config {
        let mut config_file = std::fs::File::create("config.toml")?;
        config_file.write_all(toml::to_string_pretty(config)?.as_bytes())?;
    }

    transaction.commit().await?;
    Ok(())
}
