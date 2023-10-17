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

use std::sync::Arc;

use dashmap::DashMap;

pub use crate::database_models::*;
use crate::structs::{Result, TTSMode};

type PgArguments<'a> = <sqlx::Postgres as sqlx::database::HasArguments<'a>>::Arguments;
type QueryAs<'a, R> = sqlx::query::QueryAs<'a, sqlx::Postgres, R, PgArguments<'a>>;
type Query<'a> = sqlx::query::Query<'a, sqlx::Postgres, PgArguments<'a>>;

pub trait CacheKeyTrait: std::cmp::Eq + std::hash::Hash {
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

pub struct Handler<
    CacheKey: CacheKeyTrait,
    RowT: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Compact,
> {
    pool: sqlx::PgPool,
    cache: DashMap<CacheKey, Arc<RowT::Compacted>>,

    default_row: Arc<RowT::Compacted>,
    single_insert: &'static str,
    create_row: &'static str,
    select: &'static str,
    delete: &'static str,
}

impl<CacheKey, RowT> Handler<CacheKey, RowT>
where
    CacheKey: CacheKeyTrait + Sync + Send + Copy + Default,
    RowT: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Compact + Sync + Send + Unpin,
{
    pub async fn new(
        pool: sqlx::PgPool,
        select: &'static str,
        delete: &'static str,
        create_row: &'static str,
        single_insert: &'static str,
    ) -> Result<Self> {
        Ok(Self {
            cache: DashMap::new(),
            default_row: Self::_get(&pool, CacheKey::default(), select)
                .await?
                .expect("Default row not in table!"),
            pool,
            select,
            delete,
            create_row,
            single_insert,
        })
    }

    async fn _get(
        pool: &sqlx::PgPool,
        key: CacheKey,
        select: &'static str,
    ) -> Result<Option<Arc<RowT::Compacted>>> {
        let query = key.bind_query_as(sqlx::query_as(select));
        let row: Option<RowT> = query.fetch_optional(pool).await?;
        Ok(row.map(Compact::compact).map(Arc::new))
    }

    pub async fn get(&self, identifier: CacheKey) -> Result<Arc<RowT::Compacted>> {
        if let Some(row) = self.cache.get(&identifier) {
            return Ok(row.clone());
        }

        let row = Self::_get(&self.pool, identifier, self.select)
            .await?
            .unwrap_or_else(|| self.default_row.clone());

        self.cache.insert(identifier, row.clone());
        Ok(row)
    }

    pub async fn create_row(&self, identifier: CacheKey) -> Result<()> {
        identifier
            .bind_query(sqlx::query(self.create_row))
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn set_one<Val>(
        &self,
        identifier: CacheKey,
        key: &'static str,
        value: Val,
    ) -> Result<()>
    where
        for<'a> Val: sqlx::Encode<'a, sqlx::Postgres>,
        Val: sqlx::Type<sqlx::Postgres>,
        Val: Sync + Send,
    {
        let query_raw = self.single_insert.replace("{key}", key);

        identifier
            .bind_query(sqlx::query(&query_raw))
            .bind(value)
            .execute(&self.pool)
            .await?;

        self.cache.remove(&identifier);
        Ok(())
    }

    pub async fn delete(&self, identifier: CacheKey) -> Result<()> {
        identifier
            .bind_query(sqlx::query(self.delete))
            .execute(&self.pool)
            .await?;

        self.cache.remove(&identifier);
        Ok(())
    }

    pub fn invalidate_cache(&self, identifier: &CacheKey) {
        self.cache.remove(identifier);
    }
}

#[macro_export]
macro_rules! create_db_handler {
    ($pool:expr, $table_name:literal, $id_name:literal) => {{
        const TABLE_NAME: &str = $table_name;
        const ID_NAME: &str = $id_name;

        database::Handler::new(
            $pool,
            const_format::formatcp!("SELECT * FROM {TABLE_NAME} WHERE {ID_NAME} = $1"),
            const_format::formatcp!("DELETE FROM {TABLE_NAME} WHERE {ID_NAME} = $1"),
            const_format::formatcp!(
                "INSERT INTO {TABLE_NAME}({ID_NAME}) VALUES ($1)
                ON CONFLICT ({ID_NAME}) DO NOTHING"
            ),
            const_format::formatcp!(
                "INSERT INTO {TABLE_NAME}({ID_NAME}, {{key}}) VALUES ($1, $2)
                ON CONFLICT ({ID_NAME}) DO UPDATE SET {{key}} = $2"
            ),
        )
    }};
    ($pool:expr, $table_name:literal, $id_name1:literal, $id_name2:literal) => {{
        const TABLE_NAME: &str = $table_name;
        const ID_NAME1: &str = $id_name1;
        const ID_NAME2: &str = $id_name2;

        database::Handler::new(
            $pool,
            const_format::formatcp!(
                "SELECT * FROM {TABLE_NAME} WHERE {ID_NAME1} = $1 AND {ID_NAME2} = $2"
            ),
            const_format::formatcp!(
                "DELETE FROM {TABLE_NAME} WHERE {ID_NAME1} = $1 AND {ID_NAME2} = $2"
            ),
            const_format::formatcp!(
                "INSERT INTO {TABLE_NAME}({ID_NAME1}, {ID_NAME2}) VALUES ($1, $2)
                ON CONFLICT ({ID_NAME1}, {ID_NAME2}) DO NOTHING"
            ),
            const_format::formatcp!(
                "INSERT INTO {TABLE_NAME}({ID_NAME1}, {ID_NAME2}, {{key}}) VALUES ($1, $2, $3)
                ON CONFLICT ({ID_NAME1}, {ID_NAME2}) DO UPDATE SET {{key}} = $3"
            ),
        )
    }};
}
