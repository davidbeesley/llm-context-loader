#[cfg(test)]
mod tests {
    use crate::lsp_client::{RestartableRustAnalyzerClient, RustAnalyzerClient};

    #[test]
    fn test_health_check() {
        let project_root = std::env::current_dir().unwrap();
        let mut client = RustAnalyzerClient::new(project_root).unwrap();

        // Initialize the client first
        client.initialize().unwrap();

        // Client should be healthy after creation
        assert!(client.is_healthy());

        // Properly shutdown
        client.shutdown().unwrap();
        // Note: we can't check health after shutdown because client is consumed
    }

    #[test]
    fn test_restartable_client() {
        let project_root = std::env::current_dir().unwrap();
        let mut restartable = RestartableRustAnalyzerClient::new(project_root).unwrap();

        // Should be able to get a client
        let client = restartable.get_or_restart().unwrap();
        assert!(client.is_healthy());
    }
}
