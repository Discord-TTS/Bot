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

use strfmt::strfmt;
use dashmap::DashMap;
use tokio_postgres::{Statement, Error as SqlError};
use deadpool_postgres::{tokio_postgres, Object as Connection};

use crate::structs::{Error, TTSMode};

#[poise::async_trait]
pub trait CacheKeyTrait {
    async fn get(&self, conn: Connection, stmt: Statement) -> Result<Option<tokio_postgres::Row>, SqlError>;
    async fn set_one(&self, conn: Connection, stmt: Statement, value: &(impl tokio_postgres::types::ToSql + Sync)) -> Result<u64, SqlError>;
    async fn execute(&self, conn: Connection, stmt: Statement) -> Result<u64, SqlError>;
}

#[poise::async_trait]
impl CacheKeyTrait for i64 {
    async fn get(&self, conn: Connection, stmt: Statement) -> Result<Option<tokio_postgres::Row>, SqlError> {
        conn.query_opt(&stmt, &[self]).await
    }
    async fn set_one(&self, conn: Connection, stmt: Statement, value: &(impl tokio_postgres::types::ToSql + Sync)) -> Result<u64, SqlError> {
        conn.execute(&stmt, &[self, value]).await
    }
    async fn execute(&self, conn: Connection, stmt: Statement) -> Result<u64, SqlError> {
        conn.execute(&stmt, &[self]).await
    }
}

#[poise::async_trait]
impl CacheKeyTrait for [i64; 2] {
    async fn get(&self, conn: Connection, stmt: Statement) -> Result<Option<tokio_postgres::Row>, SqlError> {
        let [guild_id, user_id] = self;
        conn.query_opt(&stmt, &[guild_id, user_id]).await
    }
    async fn set_one(&self, conn: Connection, stmt: Statement, value: &(impl tokio_postgres::types::ToSql + Sync)) -> Result<u64, SqlError> {
        let [guild_id, user_id] = self;
        conn.execute(&stmt, &[guild_id, user_id, value]).await
    }
    async fn execute(&self, conn: Connection, stmt: Statement) -> Result<u64, SqlError> {
        let [guild_id, user_id] = self;
        conn.execute(&stmt, &[guild_id, user_id]).await
    }
}

#[poise::async_trait]
impl CacheKeyTrait for (i64, TTSMode) {
    async fn get(&self, conn: Connection, stmt: Statement) -> Result<Option<tokio_postgres::Row>, SqlError> {
        let (id, mode) = self;
        conn.query_opt(&stmt, &[id, mode]).await
    }
    async fn set_one(&self, conn: Connection, stmt: Statement, value: &(impl tokio_postgres::types::ToSql + Sync)) -> Result<u64, SqlError> {
        let (id, mode) = self;
        conn.execute(&stmt, &[id, mode, value]).await
    }
    async fn execute(&self, conn: Connection, stmt: Statement) -> Result<u64, SqlError> {
        let (id, mode) = self;
        conn.execute(&stmt, &[id, mode]).await
    }
}


pub struct Handler<T: CacheKeyTrait + std::cmp::Eq + std::hash::Hash> {
    pool: Arc<deadpool_postgres::Pool>,
    cache: DashMap<T, Arc<tokio_postgres::Row>>,

    default_row: Arc<tokio_postgres::Row>,
    single_insert: &'static str,
    create_row: &'static str,
    select: &'static str,
    delete: &'static str,
}

impl<T> Handler<T>
where T: CacheKeyTrait + std::cmp::Eq + std::hash::Hash + std::marker::Sync + std::marker::Send + Copy
{
    pub async fn new(
        pool: Arc<deadpool_postgres::Pool>,
        default_id: T,
        select: &'static str,
        delete: &'static str,
        create_row: &'static str,
        single_insert: &'static str,
    ) -> Result<Self, Error> {
        Ok(Self {
            cache: DashMap::new(),
            default_row: Self::_get(
                pool.get().await?,
                select,
                default_id
            ).await?.expect("Default row not in table!"),

            pool, select, delete, create_row, single_insert,
        })
    }

    async fn _get(conn: Connection, select_query: &'static str, identifier: T) -> Result<Option<Arc<tokio_postgres::Row>>, Error> {
        let stmt = conn.prepare_cached(select_query).await?;
        Ok(identifier.get(conn, stmt).await?.map(Arc::new))
    }

    pub async fn get(&self, identifier: T) -> Result<Arc<tokio_postgres::Row>, Error> {
        if let Some(row) = self.cache.get(&identifier) {
            return Ok(row.clone());
        }

        let row = Self::_get(
            self.pool.get().await?,
            self.select,
            identifier
        ).await?.unwrap_or_else(|| self.default_row.clone());

        self.cache.insert(identifier, row.clone());
        Ok(row)
    }

    pub async fn create_row(
        &self,
        identifier: T
    ) -> Result<(), Error> {
        let conn = self.pool.get().await?;
        let stmt = conn.prepare_cached(self.create_row).await?;
        identifier.execute(conn, stmt).await?;

        Ok(())
    }

    pub async fn set_one(
        &self,
        identifier: T,
        key: &str,
        value: &(impl tokio_postgres::types::ToSql + Sync),
    ) -> Result<(), Error> {
        let conn = self.pool.get().await?;

        let stmt = conn.prepare_cached(&strfmt(self.single_insert, &{
            let mut kwargs: HashMap<String, String> = HashMap::new();
            kwargs.insert("key".to_string(), key.to_string());
            kwargs
        })?).await?;

        identifier.set_one(conn, stmt, value).await?;
        self.cache.remove(&identifier);

        Ok(())
    }

    pub async fn delete(&self, identifier: T) -> Result<(), Error> {
        let conn = self.pool.get().await?;

        let stmt = conn.prepare_cached(self.delete).await?;
        identifier.execute(conn, stmt).await?;
        self.cache.remove(&identifier);

        Ok(())
    }
}
