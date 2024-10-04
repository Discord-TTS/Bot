use std::{hash::Hash, sync::Arc};

use dashmap::DashMap;
use typesize::TypeSize;

pub use crate::database_models::*;
use crate::structs::{Result, TTSMode};

type PgArguments<'a> = <sqlx::Postgres as sqlx::database::Database>::Arguments<'a>;
type QueryAs<'a, R> = sqlx::query::QueryAs<'a, sqlx::Postgres, R, PgArguments<'a>>;
type Query<'a> = sqlx::query::Query<'a, sqlx::Postgres, PgArguments<'a>>;

pub trait CacheKeyTrait: std::cmp::Eq + Hash {
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

type OwnedArc<T> = typesize::ptr::SizableArc<T, typesize::ptr::Owned>;

pub struct Handler<CacheKey, RowT: Compact> {
    pool: sqlx::PgPool,
    cache: DashMap<CacheKey, OwnedArc<RowT::Compacted>>,

    default_row: Arc<RowT::Compacted>,
    single_insert: &'static str,
    create_row: &'static str,
    select: &'static str,
    delete: &'static str,
}

impl<CacheKey, RowT> Handler<CacheKey, RowT>
where
    CacheKey: CacheKeyTrait + Sync + Send + Copy + Default,
    RowT: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Compact + Send + Unpin,
{
    pub async fn new(
        pool: sqlx::PgPool,
        select: &'static str,
        delete: &'static str,
        create_row: &'static str,
        single_insert: &'static str,
    ) -> Result<Self> {
        let default_row = Self::_get(&pool, CacheKey::default(), select)
            .await?
            .expect("Default row not in table!");

        println!("Loaded default row for table with select: {select}");
        Ok(Self {
            cache: DashMap::new(),
            default_row,
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

        self.cache.insert(identifier, row.clone().into());
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

        self.invalidate_cache(&identifier);
        Ok(())
    }

    pub async fn delete(&self, identifier: CacheKey) -> Result<()> {
        identifier
            .bind_query(sqlx::query(self.delete))
            .execute(&self.pool)
            .await?;

        self.invalidate_cache(&identifier);
        Ok(())
    }

    pub fn invalidate_cache(&self, identifier: &CacheKey) {
        self.cache.remove(identifier);
    }
}

impl<CacheKey: Eq + Hash + TypeSize, RowT: Compact> TypeSize for Handler<CacheKey, RowT>
where
    RowT::Compacted: TypeSize,
{
    fn extra_size(&self) -> usize {
        self.cache.extra_size()
    }

    typesize::if_typesize_details! {
        fn get_collection_item_count(&self) -> Option<usize> {
            self.cache.get_collection_item_count()
        }

        fn get_size_details(&self) -> Vec<typesize::Field> {
            self.cache.get_size_details()
        }
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
