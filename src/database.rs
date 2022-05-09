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

use dashmap::DashMap;

use crate::structs::{Result, TTSMode};

type PgArguments<'a> = <sqlx::Postgres as sqlx::database::HasArguments<'a>>::Arguments;
type QueryAs<'a, R> = sqlx::query::QueryAs<'a, sqlx::Postgres, R, PgArguments<'a>>;
type Query<'a> = sqlx::query::Query<'a, sqlx::Postgres, PgArguments<'a>>;

pub trait CacheKeyTrait {
    fn bind_query(self, query: Query<'_>) -> Query<'_>;
    fn bind_query_as<R>(self, query: QueryAs<'_, R>) -> QueryAs<'_, R>;
}

impl CacheKeyTrait for i64 {
    fn bind_query(self, query: Query<'_>) -> Query<'_> {
        query.bind(self)
    }
    fn bind_query_as<R>(self, query: QueryAs<'_, R>) -> QueryAs<'_, R> {
        query.bind(self)
    }
}

impl CacheKeyTrait for [i64; 2] {
    fn bind_query(self, query: Query<'_>) -> Query<'_> {
        query.bind(self[0]).bind(self[1])
    }
    fn bind_query_as<R>(self, query: QueryAs<'_, R>) -> QueryAs<'_, R> {
        query.bind(self[0]).bind(self[1])
    }
}

impl CacheKeyTrait for (i64, TTSMode) {
    fn bind_query(self, query: Query<'_>) -> Query<'_> {
        query.bind(self.0).bind(self.1)
    }
    fn bind_query_as<R>(self, query: QueryAs<'_, R>) -> QueryAs<'_, R> {
        query.bind(self.0).bind(self.1)
    }
}


#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, sqlx::FromRow)]
pub struct GuildRow {
    pub guild_id: i64,
    pub channel: i64,
    pub premium_user: Option<i64>,
    pub xsaid: bool,
    pub auto_join: bool,
    pub bot_ignore: bool,
    pub to_translate: bool,
    pub require_voice: bool,
    pub audience_ignore: bool,
    pub msg_length: i16,
    pub repeated_chars: i16,
    pub prefix: String,
    pub target_lang: Option<String>,
    pub voice_mode: TTSMode,
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserRow {
    pub user_id: i64,
    pub dm_blocked: bool,
    pub dm_welcomed: bool,
    pub voice_mode: Option<TTSMode>,
    pub premium_voice_mode: Option<TTSMode>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct GuildVoiceRow {
    pub guild_id: i64,
    pub mode: TTSMode,
    pub voice: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserVoiceRow {
    pub user_id: i64,
    pub mode: TTSMode,
    pub voice: Option<String>,
    pub speaking_rate: Option<f32>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct NicknameRow {
    pub guild_id: i64,
    pub user_id: i64,
    pub name: Option<String>,
}


pub struct Handler<
    CacheKey: CacheKeyTrait + std::cmp::Eq + std::hash::Hash,
    RowT: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow>
> {
    pool: sqlx::PgPool,
    cache: DashMap<CacheKey, Arc<RowT>>,
    query_cache: DashMap<&'static str, &'static str>,

    default_row: Arc<RowT>,
    single_insert: &'static str,
    create_row: &'static str,
    select: &'static str,
    delete: &'static str,
}

impl<CacheKey, RowT> Handler<CacheKey, RowT>
where
    CacheKey: CacheKeyTrait + std::cmp::Eq + std::hash::Hash + Sync + Send + Copy,
    RowT: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Sync + Send + Unpin,
{
    pub async fn new(
        pool: sqlx::PgPool,
        default_id: CacheKey,
        select: &'static str,
        delete: &'static str,
        create_row: &'static str,
        single_insert: &'static str,
    ) -> Result<Self> {
        Ok(Self {
            cache: DashMap::new(),
            query_cache: DashMap::new(),
            default_row: Self::_get(&pool, default_id, select).await?.expect("Default row not in table!"),
            pool, select, delete, create_row, single_insert,
        })
    }

    async fn _get(pool: &sqlx::PgPool, key: CacheKey, select: &'static str) -> Result<Option<Arc<RowT>>> {
        let query = key.bind_query_as(sqlx::query_as(select));
        query.fetch_optional(pool).await.map(|r| r.map(Arc::new)).map_err(Into::into)
    }

    pub async fn get(&self, identifier: CacheKey) -> Result<Arc<RowT>> {
        if let Some(row) = self.cache.get(&identifier) {
            return Ok(row.clone());
        }

        let row = Self::_get(&self.pool, identifier, self.select).await?.unwrap_or_else(|| self.default_row.clone());

        self.cache.insert(identifier, row.clone());
        Ok(row)
    }

    pub async fn create_row(
        &self,
        identifier: CacheKey
    ) -> Result<()> {
        identifier
            .bind_query(sqlx::query(self.create_row))
            .execute(&self.pool).await?;

        Ok(())
    }

    pub async fn set_one<'a>(
        &self,
        identifier: CacheKey,
        key: &'static str,
        value: impl sqlx::Type<sqlx::Postgres> + sqlx::Encode<'a, sqlx::Postgres> + Sync + Send + 'a,
    ) -> Result<()> {
        let query_raw = *self.query_cache.entry(key).or_insert_with(|| {
            Box::leak(Box::new(strfmt::strfmt(
                self.single_insert,
                &HashMap::from_iter([(String::from("key"), key)])
            ).unwrap()))
        });

        identifier
            .bind_query(sqlx::query(query_raw))
            .bind(value)
            .execute(&self.pool).await?;

        self.cache.remove(&identifier);
        Ok(())
    }

    pub async fn delete(&self, identifier: CacheKey) -> Result<()> {
        identifier
            .bind_query(sqlx::query(self.delete))
            .execute(&self.pool).await?;

        self.cache.remove(&identifier);
        Ok(())
    }

    pub fn invalidate_cache(&self, identifier: &CacheKey) {
        self.cache.remove(identifier);
    }
}
