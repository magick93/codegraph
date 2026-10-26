//! Shared runtime-capable stub for the generated `cornucopia-queries`
//! crate (test scaffold).
//!
//! The real `cornucopia-queries` is produced by the cornucopia CLI from
//! the emitted annotated SQL against a live migrated Postgres, which no
//! CI lane owes compile/runtime harnesses. The stub carries the
//! `client::GenericClient` trait with the exact signature set the
//! generated `db_client.rs` implements, and — unlike a compile-only fake
//! — its deadpool impls genuinely delegate to tokio-postgres, so queries
//! EXECUTE against a real database. Consumers:
//!
//! - `workers_gateway_compile_tests` (compile gate for the gateway crate)
//! - `hooks_cornucopia_runtime_tests` (the #509/#518 hook runtime pins)
//!
//! If `templates/scaffold/db_client.tera` changes its `GenericClient`
//! surface, this stub must move with it — both consumers fail loudly if
//! the signature set drifts.

/// No-op build.rs for the stub crate (the real build.rs runs cornucopia
/// codegen).
pub const BUILD_RS: &str = "fn main() {}\n";

pub const CARGO_TOML: &str = r#"[package]
name = "cornucopia-queries"
version = "0.1.0"
edition = "2021"
publish = false

[features]
# The generated worker crates enable `deadpool` (native) / `wasm-async`
# (wasm) for their builds; the stub carries the features for resolution but
# needs no extra deps.
deadpool = ["dep:deadpool-postgres"]
wasm-async = []

[dependencies]
tokio-postgres = { version = "0.7", default-features = false, features = ["with-chrono-0_4", "with-uuid-1", "with-serde_json-1"] }
deadpool-postgres = { version = "0.14", optional = true }
"#;

pub const LIB_RS: &str = r#"//! Check-time stub (test scaffold): `client::GenericClient` with the exact
//! signature set the generated `db_client.rs` implements.

pub mod client {
    use std::future::Future;

    use tokio_postgres::types::{BorrowToSql, ToSql};
    use tokio_postgres::{Error, Row, RowStream, Statement, ToStatement};

    pub trait GenericClient: Send + Sync {
        fn stmt_cache() -> bool
        where
            Self: Sized,
        {
            true
        }

        fn prepare(&self, query: &str)
        -> impl Future<Output = Result<Statement, Error>> + Send;

        fn execute<T>(
            &self,
            query: &T,
            params: &[&(dyn ToSql + Sync)],
        ) -> impl Future<Output = Result<u64, Error>> + Send
        where
            T: ?Sized + ToStatement + Sync + Send;

        fn query_one<T>(
            &self,
            statement: &T,
            params: &[&(dyn ToSql + Sync)],
        ) -> impl Future<Output = Result<Row, Error>> + Send
        where
            T: ?Sized + ToStatement + Sync + Send;

        fn query_opt<T>(
            &self,
            statement: &T,
            params: &[&(dyn ToSql + Sync)],
        ) -> impl Future<Output = Result<Option<Row>, Error>> + Send
        where
            T: ?Sized + ToStatement + Sync + Send;

        fn query<T>(
            &self,
            query: &T,
            params: &[&(dyn ToSql + Sync)],
        ) -> impl Future<Output = Result<Vec<Row>, Error>> + Send
        where
            T: ?Sized + ToStatement + Sync + Send;

        fn query_raw<T, I>(
            &self,
            statement: &T,
            params: I,
        ) -> impl Future<Output = Result<RowStream, Error>> + Send
        where
            T: ?Sized + ToStatement + Sync + Send,
            I: IntoIterator + Sync + Send,
            I::IntoIter: ExactSizeIterator,
            I::Item: BorrowToSql;
    }

    // The real cornucopia provides GenericClient impls for the deadpool
    // handle types (the generated `DbClient::Deadpool` arms delegate to
    // them); the stub mirrors that surface by deref-delegating to Client.
    #[cfg(all(not(target_arch = "wasm32"), feature = "deadpool"))]
    impl GenericClient for deadpool_postgres::Object {
        fn stmt_cache() -> bool
        where
            Self: Sized,
        {
            false
        }

        fn prepare(
            &self,
            query: &str,
        ) -> impl Future<Output = Result<Statement, Error>> + Send {
            async move { tokio_postgres::Client::prepare(self, query).await }
        }

        fn execute<T>(
            &self,
            query: &T,
            params: &[&(dyn ToSql + Sync)],
        ) -> impl Future<Output = Result<u64, Error>> + Send
        where
            T: ?Sized + ToStatement + Sync + Send,
        {
            async move { tokio_postgres::Client::execute(self, query, params).await }
        }

        fn query_one<T>(
            &self,
            statement: &T,
            params: &[&(dyn ToSql + Sync)],
        ) -> impl Future<Output = Result<Row, Error>> + Send
        where
            T: ?Sized + ToStatement + Sync + Send,
        {
            async move { tokio_postgres::Client::query_one(self, statement, params).await }
        }

        fn query_opt<T>(
            &self,
            statement: &T,
            params: &[&(dyn ToSql + Sync)],
        ) -> impl Future<Output = Result<Option<Row>, Error>> + Send
        where
            T: ?Sized + ToStatement + Sync + Send,
        {
            async move { tokio_postgres::Client::query_opt(self, statement, params).await }
        }

        fn query<T>(
            &self,
            query: &T,
            params: &[&(dyn ToSql + Sync)],
        ) -> impl Future<Output = Result<Vec<Row>, Error>> + Send
        where
            T: ?Sized + ToStatement + Sync + Send,
        {
            async move { tokio_postgres::Client::query(self, query, params).await }
        }

        fn query_raw<T, I>(
            &self,
            statement: &T,
            params: I,
        ) -> impl Future<Output = Result<RowStream, Error>> + Send
        where
            T: ?Sized + ToStatement + Sync + Send,
            I: IntoIterator + Sync + Send,
            I::IntoIter: ExactSizeIterator,
            I::Item: BorrowToSql,
        {
            async move { tokio_postgres::Client::query_raw(self, statement, params).await }
        }
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "deadpool"))]
    impl GenericClient for deadpool_postgres::Transaction<'_> {
        fn stmt_cache() -> bool
        where
            Self: Sized,
        {
            false
        }

        fn prepare(
            &self,
            query: &str,
        ) -> impl Future<Output = Result<Statement, Error>> + Send {
            async move { tokio_postgres::Transaction::prepare(&**self, query).await }
        }

        fn execute<T>(
            &self,
            query: &T,
            params: &[&(dyn ToSql + Sync)],
        ) -> impl Future<Output = Result<u64, Error>> + Send
        where
            T: ?Sized + ToStatement + Sync + Send,
        {
            async move { tokio_postgres::Transaction::execute(&**self, query, params).await }
        }

        fn query_one<T>(
            &self,
            statement: &T,
            params: &[&(dyn ToSql + Sync)],
        ) -> impl Future<Output = Result<Row, Error>> + Send
        where
            T: ?Sized + ToStatement + Sync + Send,
        {
            async move {
                tokio_postgres::Transaction::query_one(&**self, statement, params).await
            }
        }

        fn query_opt<T>(
            &self,
            statement: &T,
            params: &[&(dyn ToSql + Sync)],
        ) -> impl Future<Output = Result<Option<Row>, Error>> + Send
        where
            T: ?Sized + ToStatement + Sync + Send,
        {
            async move {
                tokio_postgres::Transaction::query_opt(&**self, statement, params).await
            }
        }

        fn query<T>(
            &self,
            query: &T,
            params: &[&(dyn ToSql + Sync)],
        ) -> impl Future<Output = Result<Vec<Row>, Error>> + Send
        where
            T: ?Sized + ToStatement + Sync + Send,
        {
            async move { tokio_postgres::Transaction::query(&**self, query, params).await }
        }

        fn query_raw<T, I>(
            &self,
            statement: &T,
            params: I,
        ) -> impl Future<Output = Result<RowStream, Error>> + Send
        where
            T: ?Sized + ToStatement + Sync + Send,
            I: IntoIterator + Sync + Send,
            I::IntoIter: ExactSizeIterator,
            I::Item: BorrowToSql,
        {
            async move { tokio_postgres::Transaction::query_raw(&**self, statement, params).await }
        }
    }
}
"#;
