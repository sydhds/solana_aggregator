pub mod actions;
pub mod actions_account;
pub mod models;
mod schema;

use diesel::connection::SimpleConnection;
use diesel::{prelude::*, r2d2};
use std::time::Duration; // batch_execute trait

/// Short-hand for the database pool type to use throughout the app.
pub type DbPool = r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>;

#[derive(Debug)]
pub struct ConnectionOptions {
    pub enable_wal: bool,
    pub enable_foreign_keys: bool,
    pub busy_timeout: Option<Duration>,
}

impl diesel::r2d2::CustomizeConnection<SqliteConnection, diesel::r2d2::Error>
    for ConnectionOptions
{
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        // Tweak for sqlite to handle concurrent writes
        (|| {
            if self.enable_wal {
                conn.batch_execute("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
            }
            if self.enable_foreign_keys {
                conn.batch_execute("PRAGMA foreign_keys = ON;")?;
            }
            if let Some(d) = self.busy_timeout {
                conn.batch_execute(&format!("PRAGMA busy_timeout = {};", d.as_millis()))?;
            }
            Ok(())
        })()
        .map_err(r2d2::Error::QueryError)
    }
}

/// Initialize database connection pool based on `DATABASE_URL` environment variable.
pub fn initialize_db_pool(db_url: String) -> DbPool {
    r2d2::Pool::builder()
        .max_size(8)
        .connection_customizer(Box::new(ConnectionOptions {
            enable_wal: true,
            enable_foreign_keys: true,
            busy_timeout: Some(Duration::from_secs(10)),
        }))
        .build(r2d2::ConnectionManager::<SqliteConnection>::new(db_url))
        .expect("database URL should be valid path to SQLite DB file")
}
