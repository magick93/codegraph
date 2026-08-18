//! Postgres connection target shared across db/migrate/suites.

use crate::error::{OpsError, OpsResult};

/// A postgres target: host/port/user/password/database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgTarget {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub db: String,
    /// Role label used in log messages (api / e2e / e2e_app).
    pub role: String,
}

impl PgTarget {
    /// Parse a `postgres://user:pass@host:port/db` URL.
    pub fn from_url(url: &str) -> OpsResult<Self> {
        let parsed = url::Url::parse(url)
            .map_err(|e| OpsError::Config(format!("invalid database URL {url:?}: {e}")))?;
        if parsed.scheme() != "postgres" && parsed.scheme() != "postgresql" {
            return Err(OpsError::Config(format!(
                "expected postgres:// URL, got scheme {:?}",
                parsed.scheme()
            )));
        }
        let host = parsed
            .host_str()
            .ok_or_else(|| OpsError::Config(format!("missing host in {url:?}")))?
            .to_string();
        let port = parsed.port().unwrap_or(5432);
        let user = parsed.username().to_string();
        let password = parsed.password().unwrap_or("").to_string();
        let db = parsed.path().trim_start_matches('/').to_string();
        Ok(Self {
            host,
            port,
            user,
            password,
            db,
            role: "default".to_string(),
        })
    }

    /// Build a `postgres://...` URL for tools that need one.
    pub fn url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.user, self.password, self.host, self.port, self.db
        )
    }

    /// Standard connection args for `psql` (host/port/user/db), without
    /// password (supplied via `PGPASSWORD` env).
    pub fn psql_args(&self) -> Vec<String> {
        vec![
            "-h".into(),
            self.host.clone(),
            "-p".into(),
            self.port.to_string(),
            "-U".into(),
            self.user.clone(),
            "-d".into(),
            self.db.clone(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_postgres_url() {
        let t = PgTarget::from_url("postgres://u:pw@localhost:5433/mydb").unwrap();
        assert_eq!(t.user, "u");
        assert_eq!(t.password, "pw");
        assert_eq!(t.host, "localhost");
        assert_eq!(t.port, 5433);
        assert_eq!(t.db, "mydb");
        assert_eq!(t.url(), "postgres://u:pw@localhost:5433/mydb");
    }

    #[test]
    fn defaults_port_when_missing() {
        let t = PgTarget::from_url("postgres://u@localhost/mydb").unwrap();
        assert_eq!(t.port, 5432);
        assert_eq!(t.password, "");
    }

    #[test]
    fn rejects_non_postgres_scheme() {
        assert!(PgTarget::from_url("mysql://u@h/db").is_err());
    }
}
