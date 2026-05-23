use crate::schema_ddl;
use grafeo::GrafeoDB;
use codegraph_core::error::GraphError;
use std::sync::Arc;
use std::time::Instant;

pub struct GrafeoEngine {
    db: Arc<GrafeoDB>,
    start_time: Instant,
}

impl GrafeoEngine {
    /// Create a new in-memory Grafeo instance with typed schema DDL.
    pub fn in_memory() -> Result<Self, GraphError> {
        let db = GrafeoDB::new_in_memory();
        let engine = Self {
            db: Arc::new(db),
            start_time: Instant::now(),
        };
        engine.init_schema()?;
        Ok(engine)
    }

    /// Create a persistent Grafeo instance using a config.
    pub fn with_config(config: grafeo::Config) -> Result<Self, GraphError> {
        let db = GrafeoDB::with_config(config)
            .map_err(|e| GraphError::Connection(format!("config open failed: {e}")))?;
        let engine = Self {
            db: Arc::new(db),
            start_time: Instant::now(),
        };
        engine.init_schema()?;
        Ok(engine)
    }

    /// Re-run schema DDL (idempotent due to IF NOT EXISTS).
    pub fn reinit_schema(&self) -> Result<(), GraphError> {
        self.init_schema()
    }

    /// Get a reference to the underlying database.
    pub fn db(&self) -> &GrafeoDB {
        &self.db
    }

    /// Get the start time for duration tracking in finalize().
    pub(crate) fn start_time(&self) -> Instant {
        self.start_time
    }

    fn init_schema(&self) -> Result<(), GraphError> {
        let session = self.db.session();
        for ddl in schema_ddl::ddl_statements() {
            session
                .execute(ddl)
                .map_err(|e| GraphError::Ingest(format!("DDL failed: {e}")))?;
        }
        // Use programmatic API for indexes (GQL CREATE INDEX has parser issues with FOR keyword)
        for prop in schema_ddl::indexed_properties() {
            self.db.create_property_index(prop);
        }
        Ok(())
    }
}
