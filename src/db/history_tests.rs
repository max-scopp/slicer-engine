#[cfg(test)]
mod history_integration_tests {
    use crate::ws_protocol::{ClientMessage, ServerMessage, SessionSummary};
    use crate::db::{Database, RequestStatus};
    use tempfile::TempDir;
    use uuid::Uuid;

    #[test]
    fn test_list_sessions_message_serialization() {
        let summaries = vec![SessionSummary {
            request_uuid: "test-uuid".to_string(),
            original_filename: Some("model.stl".to_string()),
            layer_count: Some(100),
            created_at: "2026-04-26T00:00:00Z".to_string(),
            download_url: "/api/download/test-uuid".to_string(),
        }];

        let msg = ServerMessage::SessionsList { sessions: summaries };
        
        // Verify message can be serialized to JSON
        let json = serde_json::to_string(&msg).expect("Should serialize");
        assert!(json.contains("SessionsList"));
        assert!(json.contains("model.stl"));
        assert!(json.contains("100"));
    }

    #[test]
    fn test_list_sessions_request_message() {
        let msg = ClientMessage::ListSessions;
        
        // Verify message can be serialized to JSON
        let json = serde_json::to_string(&msg).expect("Should serialize");
        assert!(json.contains("ListSessions"));
    }

    #[test]
    fn test_completed_sessions_query() -> Result<(), Box<dyn std::error::Error>> {
        let dir = TempDir::new()?;
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path)?;

        let uuid1 = Uuid::new_v4();
        let uuid2 = Uuid::new_v4();

        // Create two sessions
        db.create_request(uuid1)?;
        db.create_request(uuid2)?;

        // Mark both as complete
        db.update_status(uuid1, RequestStatus::SliceComplete)?;
        db.update_status(uuid2, RequestStatus::SliceComplete)?;

        // Query completed sessions
        let completed = db.get_completed_sessions()?;
        
        // Should have at least 2 completed sessions
        assert!(completed.len() >= 2);
        
        // Verify sessions are SliceComplete status
        for session in &completed {
            assert_eq!(session.status, RequestStatus::SliceComplete);
        }

        Ok(())
    }
}
