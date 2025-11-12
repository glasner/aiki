use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

use crate::provenance::{AgentType, AttributionConfidence, DetectionMethod, ProvenanceRecord};

/// Database for storing provenance records
pub struct ProvenanceDatabase {
    conn: Connection,
}

impl ProvenanceDatabase {
    /// Open or create the provenance database
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)
            .with_context(|| format!("Failed to open database at {}", db_path.display()))?;

        Ok(Self { conn })
    }

    /// Initialize the database schema
    pub fn initialize(&self) -> Result<()> {
        self.conn
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS provenance_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                agent_type TEXT NOT NULL,
                agent_version TEXT,
                detected_at TEXT NOT NULL,
                confidence TEXT NOT NULL,
                detection_method TEXT NOT NULL,
                file_path TEXT NOT NULL,
                session_id TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                old_string TEXT,
                new_string TEXT,
                jj_commit_id TEXT,
                jj_operation_id TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_session_id ON provenance_records(session_id);
            CREATE INDEX IF NOT EXISTS idx_file_path ON provenance_records(file_path);
            CREATE INDEX IF NOT EXISTS idx_timestamp ON provenance_records(timestamp);
            CREATE INDEX IF NOT EXISTS idx_jj_commit_id ON provenance_records(jj_commit_id);
            CREATE INDEX IF NOT EXISTS idx_jj_operation_id ON provenance_records(jj_operation_id);
            "#,
            )
            .context("Failed to create database schema")?;

        Ok(())
    }

    /// Insert a provenance record and return its ID
    pub fn insert_provenance(&self, record: &ProvenanceRecord) -> Result<i64> {
        let agent_type = match record.agent.agent_type {
            AgentType::ClaudeCode => "ClaudeCode",
            AgentType::Unknown => "Unknown",
        };

        let confidence = match record.agent.confidence {
            AttributionConfidence::High => "High",
            AttributionConfidence::Medium => "Medium",
            AttributionConfidence::Low => "Low",
            AttributionConfidence::Unknown => "Unknown",
        };

        let detection_method = match record.agent.detection_method {
            DetectionMethod::Hook => "Hook",
            DetectionMethod::Unknown => "Unknown",
        };

        let old_string = record
            .change_summary
            .as_ref()
            .and_then(|cs| cs.old_string.as_deref());

        let new_string = record
            .change_summary
            .as_ref()
            .and_then(|cs| cs.new_string.as_deref());

        self.conn
            .execute(
                r#"
            INSERT INTO provenance_records (
                agent_type, agent_version, detected_at, confidence, detection_method,
                file_path, session_id, tool_name, timestamp,
                old_string, new_string, jj_commit_id, jj_operation_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
                params![
                    agent_type,
                    record.agent.version.as_deref(),
                    record.agent.detected_at.to_rfc3339(),
                    confidence,
                    detection_method,
                    record.file_path.to_string_lossy(),
                    record.session_id,
                    record.tool_name,
                    record.timestamp.to_rfc3339(),
                    old_string,
                    new_string,
                    record.jj_commit_id.as_deref(),
                    record.jj_operation_id.as_deref(),
                ],
            )
            .context("Failed to insert provenance record")?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Update the JJ operation ID for a provenance record
    /// (will be used by op_heads watcher in Milestone 1.2)
    #[allow(dead_code)]
    pub fn update_operation_id(&self, record_id: i64, operation_id: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE provenance_records SET jj_operation_id = ?1 WHERE id = ?2",
                params![operation_id, record_id],
            )
            .context("Failed to update operation ID")?;

        Ok(())
    }
}

/// Initialize the provenance database directory and file
pub fn initialize_provenance_db(repo_path: &Path) -> Result<()> {
    let provenance_dir = repo_path.join(".aiki").join("provenance");
    std::fs::create_dir_all(&provenance_dir)
        .context("Failed to create .aiki/provenance directory")?;

    let db_path = provenance_dir.join("attribution.db");
    let db = ProvenanceDatabase::open(&db_path)?;
    db.initialize()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::{AgentInfo, ChangeSummary};
    use chrono::Utc;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_database_initialization() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = ProvenanceDatabase::open(&db_path).unwrap();
        db.initialize().unwrap();

        // Database file should exist
        assert!(db_path.exists());
    }

    #[test]
    fn test_insert_provenance_record() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = ProvenanceDatabase::open(&db_path).unwrap();
        db.initialize().unwrap();

        let record = ProvenanceRecord {
            id: None,
            agent: AgentInfo {
                agent_type: AgentType::ClaudeCode,
                version: Some("1.0".to_string()),
                detected_at: Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            file_path: PathBuf::from("/test/file.rs"),
            session_id: "session123".to_string(),
            tool_name: "Edit".to_string(),
            timestamp: Utc::now(),
            change_summary: Some(ChangeSummary {
                old_string: Some("old".to_string()),
                new_string: Some("new".to_string()),
            }),
            jj_commit_id: Some("abc123".to_string()),
            jj_operation_id: None,
        };

        let id = db.insert_provenance(&record).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_update_operation_id() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = ProvenanceDatabase::open(&db_path).unwrap();
        db.initialize().unwrap();

        let record = ProvenanceRecord {
            id: None,
            agent: AgentInfo {
                agent_type: AgentType::ClaudeCode,
                version: None,
                detected_at: Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            file_path: PathBuf::from("/test/file.rs"),
            session_id: "session123".to_string(),
            tool_name: "Write".to_string(),
            timestamp: Utc::now(),
            change_summary: None,
            jj_commit_id: Some("abc123".to_string()),
            jj_operation_id: None,
        };

        let id = db.insert_provenance(&record).unwrap();
        let result = db.update_operation_id(id, "op456");
        assert!(result.is_ok());
    }

    #[test]
    fn test_initialize_provenance_db() {
        let temp_dir = tempdir().unwrap();
        let repo_path = temp_dir.path();

        // Create .aiki directory first
        std::fs::create_dir_all(repo_path.join(".aiki")).unwrap();

        let result = initialize_provenance_db(repo_path);
        assert!(result.is_ok());

        // Check directory and database exist
        assert!(repo_path.join(".aiki/provenance").exists());
        assert!(repo_path.join(".aiki/provenance/attribution.db").exists());
    }
}
