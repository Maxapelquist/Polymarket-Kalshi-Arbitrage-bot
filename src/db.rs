//! SQLite persistence layer for market discovery, pricing history, and trade logs.
//!
//! ## Database Schema
//!
//! This module manages a local SQLite database with the following tables:
//!
//! - `series`: Kalshi series (e.g., "NBA", "EPL", "PRES2024")
//! - `events`: Kalshi events (e.g., specific game or question)
//! - `markets`: Individual binary markets from both platforms
//! - `prices`: Historical price snapshots (delta-only writes)
//! - `matches`: Semantic matches between Kalshi and Polymarket
//! - `trades`: Execution log for all arbitrage attempts
//!
//! ## Design Principles
//!
//! 1. **Hot path isolation**: Price updates in memory (atomic), periodic DB writes
//! 2. **Deduplication**: Events/markets stored once, prevents redundant API calls
//! 3. **Audit trail**: All discoveries, matches, and trades are logged
//! 4. **Fast queries**: Indexes on tickers and timestamps
//!
//! ## GOLDEN RULE
//!
//! NEVER bet unless:
//! - events are semantically identical
//! - outcomes fully cover sample space
//! - profit is positive after fees

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

/// Thread-safe database connection wrapper
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Open or create the database with WAL mode for better concurrency
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path.as_ref())
            .context("Failed to open database")?;

        // Enable WAL mode for better concurrency
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        db.init_schema()?;
        Ok(db)
    }

    /// Initialize database schema if tables don't exist
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            r#"
            -- Kalshi series (top-level category)
            CREATE TABLE IF NOT EXISTS series (
                series_ticker TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                category TEXT,
                created_at INTEGER NOT NULL DEFAULT (unixepoch())
            );

            -- Kalshi events (specific question or game)
            CREATE TABLE IF NOT EXISTS events (
                event_ticker TEXT PRIMARY KEY,
                series_ticker TEXT NOT NULL,
                title TEXT NOT NULL,
                subtitle TEXT,
                open_time INTEGER,
                close_time INTEGER,
                status TEXT,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                FOREIGN KEY (series_ticker) REFERENCES series(series_ticker)
            );

            CREATE INDEX IF NOT EXISTS idx_events_series ON events(series_ticker);
            CREATE INDEX IF NOT EXISTS idx_events_close_time ON events(close_time);

            -- Markets from both platforms
            CREATE TABLE IF NOT EXISTS markets (
                market_id INTEGER PRIMARY KEY AUTOINCREMENT,
                platform TEXT NOT NULL CHECK(platform IN ('kalshi', 'polymarket')),
                
                -- Kalshi fields
                event_ticker TEXT,
                market_ticker TEXT,
                
                -- Polymarket fields
                poly_slug TEXT,
                poly_condition_id TEXT,
                poly_token_id TEXT,
                
                -- Common fields
                title TEXT NOT NULL,
                subtitle TEXT,
                rules TEXT,
                outcome TEXT,  -- 'YES' or 'NO' for Polymarket
                
                -- Market info
                volume INTEGER,
                liquidity INTEGER,
                expiration INTEGER,
                
                -- Timestamps
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                last_seen INTEGER NOT NULL DEFAULT (unixepoch()),
                
                FOREIGN KEY (event_ticker) REFERENCES events(event_ticker)
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_markets_kalshi_ticker ON markets(market_ticker) WHERE market_ticker IS NOT NULL;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_markets_poly_token ON markets(poly_token_id) WHERE poly_token_id IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_markets_event ON markets(event_ticker);
            CREATE INDEX IF NOT EXISTS idx_markets_platform ON markets(platform);

            -- Price snapshots (delta-only writes)
            CREATE TABLE IF NOT EXISTS prices (
                price_id INTEGER PRIMARY KEY AUTOINCREMENT,
                market_id INTEGER NOT NULL,
                timestamp INTEGER NOT NULL DEFAULT (unixepoch()),
                yes_ask INTEGER,  -- in cents (0-99)
                yes_bid INTEGER,
                no_ask INTEGER,
                no_bid INTEGER,
                yes_size INTEGER,
                no_size INTEGER,
                FOREIGN KEY (market_id) REFERENCES markets(market_id)
            );

            CREATE INDEX IF NOT EXISTS idx_prices_market_time ON prices(market_id, timestamp DESC);

            -- Semantic matches between platforms
            CREATE TABLE IF NOT EXISTS matches (
                match_id INTEGER PRIMARY KEY AUTOINCREMENT,
                kalshi_market_id INTEGER NOT NULL,
                poly_market_id INTEGER NOT NULL,
                
                -- Matching metadata
                similarity_score REAL NOT NULL,
                match_method TEXT NOT NULL,  -- 'semantic', 'rule_based', 'manual'
                confidence REAL NOT NULL,
                
                -- Canonical form (AI-generated)
                canonical_json TEXT,  -- JSON: {asset, condition, threshold, date, location}
                
                -- Status
                status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active', 'expired', 'invalid', 'manual_override')),
                
                -- Timestamps
                matched_at INTEGER NOT NULL DEFAULT (unixepoch()),
                last_verified INTEGER NOT NULL DEFAULT (unixepoch()),
                
                FOREIGN KEY (kalshi_market_id) REFERENCES markets(market_id),
                FOREIGN KEY (poly_market_id) REFERENCES markets(market_id),
                UNIQUE(kalshi_market_id, poly_market_id)
            );

            CREATE INDEX IF NOT EXISTS idx_matches_kalshi ON matches(kalshi_market_id);
            CREATE INDEX IF NOT EXISTS idx_matches_poly ON matches(poly_market_id);
            CREATE INDEX IF NOT EXISTS idx_matches_status ON matches(status);

            -- Trade execution log
            CREATE TABLE IF NOT EXISTS trades (
                trade_id INTEGER PRIMARY KEY AUTOINCREMENT,
                match_id INTEGER NOT NULL,
                
                -- Detection
                detected_at INTEGER NOT NULL,
                arb_type TEXT NOT NULL,  -- 'poly_yes_kalshi_no', 'kalshi_yes_poly_no', etc.
                
                -- Prices at detection (cents)
                yes_price INTEGER NOT NULL,
                no_price INTEGER NOT NULL,
                yes_size INTEGER NOT NULL,
                no_size INTEGER NOT NULL,
                
                -- Expected profit
                expected_profit_cents INTEGER NOT NULL,
                estimated_fee_cents INTEGER NOT NULL,
                
                -- Execution
                executed BOOLEAN NOT NULL DEFAULT 0,
                execution_time INTEGER,
                kalshi_order_id TEXT,
                poly_order_hash TEXT,
                
                -- Actual results
                kalshi_filled_qty INTEGER,
                poly_filled_qty INTEGER,
                actual_profit_cents INTEGER,
                
                -- Errors
                error TEXT,
                
                FOREIGN KEY (match_id) REFERENCES matches(match_id)
            );

            CREATE INDEX IF NOT EXISTS idx_trades_match ON trades(match_id);
            CREATE INDEX IF NOT EXISTS idx_trades_detected ON trades(detected_at DESC);
            CREATE INDEX IF NOT EXISTS idx_trades_executed ON trades(executed);

            -- Metadata table for schema version and config
            CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            INSERT OR IGNORE INTO metadata (key, value) VALUES ('schema_version', '1');
            INSERT OR IGNORE INTO metadata (key, value) VALUES ('created_at', unixepoch());
            "#
        )?;

        info!("📊 Database schema initialized");
        Ok(())
    }

    // =======================================================================
    // SERIES Operations
    // =======================================================================

    /// Insert or update a Kalshi series
    pub fn upsert_series(&self, series_ticker: &str, title: &str, category: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO series (series_ticker, title, category) VALUES (?1, ?2, ?3)
             ON CONFLICT(series_ticker) DO UPDATE SET title=excluded.title, category=excluded.category",
            params![series_ticker, title, category],
        )?;
        Ok(())
    }

    // =======================================================================
    // EVENTS Operations
    // =======================================================================

    /// Insert or update a Kalshi event
    pub fn upsert_event(
        &self,
        event_ticker: &str,
        series_ticker: &str,
        title: &str,
        subtitle: Option<&str>,
        open_time: Option<i64>,
        close_time: Option<i64>,
        status: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO events (event_ticker, series_ticker, title, subtitle, open_time, close_time, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(event_ticker) DO UPDATE SET
                 title=excluded.title,
                 subtitle=excluded.subtitle,
                 open_time=excluded.open_time,
                 close_time=excluded.close_time,
                 status=excluded.status",
            params![event_ticker, series_ticker, title, subtitle, open_time, close_time, status],
        )?;
        Ok(())
    }

    /// Check if an event already exists (for deduplication)
    pub fn event_exists(&self, event_ticker: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM events WHERE event_ticker = ?1",
                params![event_ticker],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        Ok(exists)
    }

    // =======================================================================
    // MARKETS Operations
    // =======================================================================

    /// Insert a Kalshi market
    pub fn insert_kalshi_market(
        &self,
        event_ticker: &str,
        market_ticker: &str,
        title: &str,
        subtitle: Option<&str>,
        rules: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO markets (platform, event_ticker, market_ticker, title, subtitle, rules)
             VALUES ('kalshi', ?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(market_ticker) DO UPDATE SET last_seen=unixepoch()",
            params![event_ticker, market_ticker, title, subtitle, rules],
        )?;
        
        let market_id = conn.last_insert_rowid();
        Ok(market_id)
    }

    /// Insert a Polymarket market
    pub fn insert_poly_market(
        &self,
        poly_slug: &str,
        poly_condition_id: &str,
        poly_token_id: &str,
        title: &str,
        outcome: &str,  // "YES" or "NO"
        expiration: Option<i64>,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO markets (platform, poly_slug, poly_condition_id, poly_token_id, title, outcome, expiration)
             VALUES ('polymarket', ?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(poly_token_id) DO UPDATE SET last_seen=unixepoch()",
            params![poly_slug, poly_condition_id, poly_token_id, title, outcome, expiration],
        )?;
        
        let market_id = conn.last_insert_rowid();
        Ok(market_id)
    }

    /// Get market_id by Kalshi ticker
    pub fn get_market_id_by_kalshi_ticker(&self, market_ticker: &str) -> Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        let id = conn
            .query_row(
                "SELECT market_id FROM markets WHERE market_ticker = ?1",
                params![market_ticker],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id)
    }

    /// Get market_id by Polymarket token
    pub fn get_market_id_by_poly_token(&self, poly_token_id: &str) -> Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        let id = conn
            .query_row(
                "SELECT market_id FROM markets WHERE poly_token_id = ?1",
                params![poly_token_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id)
    }

    // =======================================================================
    // MATCHES Operations
    // =======================================================================

    /// Insert a semantic match
    pub fn insert_match(
        &self,
        kalshi_market_id: i64,
        poly_market_id: i64,
        similarity_score: f32,
        match_method: &str,
        confidence: f32,
        canonical_json: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO matches (kalshi_market_id, poly_market_id, similarity_score, match_method, confidence, canonical_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(kalshi_market_id, poly_market_id) DO UPDATE SET
                 similarity_score=excluded.similarity_score,
                 confidence=excluded.confidence,
                 last_verified=unixepoch()",
            params![kalshi_market_id, poly_market_id, similarity_score, match_method, confidence, canonical_json],
        )?;
        
        let match_id = conn.last_insert_rowid();
        Ok(match_id)
    }

    /// Check if a match already exists
    pub fn match_exists(&self, kalshi_market_id: i64, poly_market_id: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM matches WHERE kalshi_market_id = ?1 AND poly_market_id = ?2 AND status = 'active'",
                params![kalshi_market_id, poly_market_id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        Ok(exists)
    }

    /// Get all active matches
    pub fn get_active_matches(&self) -> Result<Vec<(i64, i64, i64)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT match_id, kalshi_market_id, poly_market_id FROM matches WHERE status = 'active'"
        )?;
        
        let matches = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        
        Ok(matches)
    }

    // =======================================================================
    // PRICES Operations
    // =======================================================================

    /// Log a price snapshot (delta only)
    pub fn log_price(
        &self,
        market_id: i64,
        yes_ask: Option<u16>,
        yes_bid: Option<u16>,
        no_ask: Option<u16>,
        no_bid: Option<u16>,
        yes_size: Option<u16>,
        no_size: Option<u16>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO prices (market_id, yes_ask, yes_bid, no_ask, no_bid, yes_size, no_size)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![market_id, yes_ask, yes_bid, no_ask, no_bid, yes_size, no_size],
        )?;
        Ok(())
    }

    // =======================================================================
    // TRADES Operations
    // =======================================================================

    /// Log a trade attempt
    #[allow(clippy::too_many_arguments)]
    pub fn log_trade(
        &self,
        match_id: i64,
        arb_type: &str,
        yes_price: u16,
        no_price: u16,
        yes_size: u16,
        no_size: u16,
        expected_profit_cents: i16,
        estimated_fee_cents: u16,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;
        
        conn.execute(
            "INSERT INTO trades (match_id, detected_at, arb_type, yes_price, no_price, yes_size, no_size, expected_profit_cents, estimated_fee_cents)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![match_id, now, arb_type, yes_price, no_price, yes_size, no_size, expected_profit_cents, estimated_fee_cents],
        )?;
        
        let trade_id = conn.last_insert_rowid();
        debug!("📝 Logged trade #{} for match #{}", trade_id, match_id);
        Ok(trade_id)
    }

    /// Update trade execution status
    pub fn update_trade_execution(
        &self,
        trade_id: i64,
        kalshi_order_id: Option<&str>,
        poly_order_hash: Option<&str>,
        kalshi_filled_qty: Option<u16>,
        poly_filled_qty: Option<u16>,
        actual_profit_cents: Option<i16>,
        error: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;
        
        conn.execute(
            "UPDATE trades SET
                executed = 1,
                execution_time = ?1,
                kalshi_order_id = ?2,
                poly_order_hash = ?3,
                kalshi_filled_qty = ?4,
                poly_filled_qty = ?5,
                actual_profit_cents = ?6,
                error = ?7
             WHERE trade_id = ?8",
            params![now, kalshi_order_id, poly_order_hash, kalshi_filled_qty, poly_filled_qty, actual_profit_cents, error, trade_id],
        )?;
        
        Ok(())
    }

    // =======================================================================
    // STATS & REPORTING
    // =======================================================================

    /// Get database statistics
    pub fn get_stats(&self) -> Result<DbStats> {
        let conn = self.conn.lock().unwrap();
        
        let series_count: i64 = conn.query_row("SELECT COUNT(*) FROM series", [], |row| row.get(0))?;
        let events_count: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        let markets_count: i64 = conn.query_row("SELECT COUNT(*) FROM markets", [], |row| row.get(0))?;
        let active_matches: i64 = conn.query_row("SELECT COUNT(*) FROM matches WHERE status = 'active'", [], |row| row.get(0))?;
        let total_trades: i64 = conn.query_row("SELECT COUNT(*) FROM trades", [], |row| row.get(0))?;
        let executed_trades: i64 = conn.query_row("SELECT COUNT(*) FROM trades WHERE executed = 1", [], |row| row.get(0))?;
        
        Ok(DbStats {
            series_count,
            events_count,
            markets_count,
            active_matches,
            total_trades,
            executed_trades,
        })
    }
}

/// Database statistics
#[derive(Debug)]
pub struct DbStats {
    pub series_count: i64,
    pub events_count: i64,
    pub markets_count: i64,
    pub active_matches: i64,
    pub total_trades: i64,
    pub executed_trades: i64,
}

impl std::fmt::Display for DbStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "📊 DB Stats: {} series, {} events, {} markets, {} active matches, {}/{} trades executed",
            self.series_count, self.events_count, self.markets_count,
            self.active_matches, self.executed_trades, self.total_trades
        )
    }
}
