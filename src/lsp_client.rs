use anyhow::{Context, Result};
use lsp_types::{
    ClientCapabilities, DidOpenTextDocumentParams, InitializeParams, InitializeResult,
    InitializedParams, TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams,
    WorkspaceFolder,
};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use url::Url;

/// LSP client for communicating with rust-analyzer
pub struct RustAnalyzerClient {
    process: std::process::Child,
    request_id: AtomicU64,
    reader: Arc<Mutex<BufReader<std::process::ChildStdout>>>,
    writer: Arc<Mutex<std::process::ChildStdin>>,
    project_root: PathBuf,
}

impl RustAnalyzerClient {
    /// Create a new client and start rust-analyzer
    pub fn new(project_root: PathBuf) -> Result<Self> {
        let mut process = Command::new("rust-analyzer")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to start rust-analyzer. Is it installed?")?;

        let stdin = process
            .stdin
            .take()
            .context("Failed to get rust-analyzer stdin")?;
        let stdout = process
            .stdout
            .take()
            .context("Failed to get rust-analyzer stdout")?;

        let reader = Arc::new(Mutex::new(BufReader::new(stdout)));
        let writer = Arc::new(Mutex::new(stdin));

        Ok(Self {
            process,
            request_id: AtomicU64::new(1),
            reader,
            writer,
            project_root,
        })
    }

    /// Initialize the LSP connection
    pub fn initialize(&self) -> Result<InitializeResult> {
        let uri = Url::from_file_path(&self.project_root)
            .map_err(|_| anyhow::anyhow!("Invalid project root path"))?;

        let workspace_folder = WorkspaceFolder {
            uri: uri.as_str().parse().unwrap(),
            name: self
                .project_root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("workspace")
                .to_string(),
        };

        #[allow(deprecated)]
        let params = InitializeParams {
            process_id: Some(std::process::id()),
            root_path: None,
            root_uri: None,
            initialization_options: Some(json!({
                "cargo": {
                    "runBuildScripts": {
                        "enable": false
                    }
                }
            })),
            capabilities: ClientCapabilities::default(),
            trace: None,
            workspace_folders: Some(vec![workspace_folder]),
            client_info: Some(lsp_types::ClientInfo {
                name: "llm-context-loader".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            locale: None,
            work_done_progress_params: Default::default(),
        };

        let response = self.send_request("initialize", params)?;

        // Send initialized notification
        self.send_notification("initialized", InitializedParams {})?;

        serde_json::from_value(response).context("Failed to parse initialize response")
    }

    /// Open a text document
    pub fn open_document(&self, file_path: &PathBuf, content: String) -> Result<()> {
        let uri =
            Url::from_file_path(file_path).map_err(|_| anyhow::anyhow!("Invalid file path"))?;

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.as_str().parse().unwrap(),
                language_id: "rust".to_string(),
                version: 1,
                text: content,
            },
        };

        self.send_notification("textDocument/didOpen", params)
    }

    /// Get folding ranges for a document
    pub fn get_folding_ranges(&self, file_path: &PathBuf) -> Result<Vec<lsp_types::FoldingRange>> {
        let uri =
            Url::from_file_path(file_path).map_err(|_| anyhow::anyhow!("Invalid file path"))?;

        let params = lsp_types::FoldingRangeParams {
            text_document: TextDocumentIdentifier {
                uri: uri.as_str().parse().unwrap(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let response = self.send_request("textDocument/foldingRange", params)?;
        serde_json::from_value(response).context("Failed to parse folding ranges")
    }

    /// Go to definition
    pub fn goto_definition(
        &self,
        file_path: &PathBuf,
        line: u32,
        character: u32,
    ) -> Result<lsp_types::GotoDefinitionResponse> {
        let uri =
            Url::from_file_path(file_path).map_err(|_| anyhow::anyhow!("Invalid file path"))?;

        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: uri.as_str().parse().unwrap(),
            },
            position: lsp_types::Position { line, character },
        };

        let response = self.send_request("textDocument/definition", params)?;
        serde_json::from_value(response).context("Failed to parse goto definition response")
    }

    /// Find all references to a symbol
    pub fn find_references(
        &self,
        file_path: &PathBuf,
        line: u32,
        character: u32,
        include_declaration: bool,
    ) -> Result<Vec<lsp_types::Location>> {
        let uri =
            Url::from_file_path(file_path).map_err(|_| anyhow::anyhow!("Invalid file path"))?;

        let params = lsp_types::ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: uri.as_str().parse().unwrap(),
                },
                position: lsp_types::Position { line, character },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: lsp_types::ReferenceContext {
                include_declaration,
            },
        };

        let response = self.send_request("textDocument/references", params)?;
        serde_json::from_value(response).context("Failed to parse references")
    }

    /// Send a request and wait for response
    fn send_request<P: serde::Serialize>(&self, method: &str, params: P) -> Result<Value> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        self.send_message(&request)?;

        // Read response
        let response = self.read_response(id)?;

        if let Some(error) = response.get("error") {
            anyhow::bail!("LSP error: {}", error);
        }

        response
            .get("result")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No result in response"))
    }

    /// Send a notification (no response expected)
    fn send_notification<P: serde::Serialize>(&self, method: &str, params: P) -> Result<()> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        self.send_message(&notification)
    }

    /// Send a message to rust-analyzer
    fn send_message(&self, message: &Value) -> Result<()> {
        let content = serde_json::to_string(message)?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        let mut writer = self.writer.lock().unwrap();
        writer.write_all(header.as_bytes())?;
        writer.write_all(content.as_bytes())?;
        writer.flush()?;

        Ok(())
    }

    /// Read a response with the given ID
    fn read_response(&self, expected_id: u64) -> Result<Value> {
        let mut reader = self.reader.lock().unwrap();

        loop {
            // Read header
            let mut header = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line)?;
                if line == "\r\n" {
                    break;
                }
                header.push_str(&line);
            }

            // Parse content length
            let content_length = header
                .lines()
                .find(|line| line.starts_with("Content-Length:"))
                .and_then(|line| line.split(':').nth(1))
                .and_then(|len| len.trim().parse::<usize>().ok())
                .ok_or_else(|| anyhow::anyhow!("Invalid LSP header"))?;

            // Read content
            let mut content = vec![0; content_length];
            reader.read_exact(&mut content)?;

            let message: Value = serde_json::from_slice(&content)?;

            // Check if this is our response
            if let Some(id) = message.get("id").and_then(|id| id.as_u64()) {
                if id == expected_id {
                    return Ok(message);
                }
            }

            // Handle progress notifications
            if message.get("method").and_then(|m| m.as_str()) == Some("$/progress") {
                if let Some(params) = message.get("params") {
                    if let (Some(token), Some(value)) = (params.get("token"), params.get("value")) {
                        if let Some(message_text) = value.get("message").and_then(|m| m.as_str()) {
                            println!("[Progress] {}: {}", 
                                token.as_str().unwrap_or("unknown"),
                                message_text
                            );
                            if let Some(percentage) = value.get("percentage").and_then(|p| p.as_u64()) {
                                println!("  {}% complete", percentage);
                            }
                        }
                    }
                }
            }

            // Otherwise, it's a notification or different response, continue
        }
    }

    /// Get document symbols
    pub fn document_symbols(&self, file_path: &PathBuf) -> Result<Option<lsp_types::DocumentSymbolResponse>> {
        let uri = Url::from_file_path(file_path)
            .map_err(|_| anyhow::anyhow!("Invalid file path"))?;

        let params = lsp_types::DocumentSymbolParams {
            text_document: TextDocumentIdentifier {
                uri: uri.as_str().parse().unwrap(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let response = self.send_request("textDocument/documentSymbol", params)?;
        Ok(serde_json::from_value(response).ok())
    }

    /// Wait for rust-analyzer to be ready by polling for document symbols
    pub fn wait_for_ready(&self, file_path: &PathBuf, max_wait_secs: u64) -> Result<()> {
        let start = std::time::Instant::now();
        let max_duration = std::time::Duration::from_secs(max_wait_secs);
        
        println!("Waiting for rust-analyzer to analyze the file...");
        
        loop {
            // Try to get document symbols
            if let Ok(Some(symbols)) = self.document_symbols(file_path) {
                match &symbols {
                    lsp_types::DocumentSymbolResponse::Flat(symbols) if !symbols.is_empty() => {
                        println!("rust-analyzer is ready! Found {} symbols", symbols.len());
                        return Ok(());
                    }
                    lsp_types::DocumentSymbolResponse::Nested(symbols) if !symbols.is_empty() => {
                        println!("rust-analyzer is ready! Found {} symbols", symbols.len());
                        return Ok(());
                    }
                    _ => {}
                }
            }
            
            if start.elapsed() > max_duration {
                return Err(anyhow::anyhow!("Timeout waiting for rust-analyzer to be ready"));
            }
            
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    /// Shutdown the LSP server
    pub fn shutdown(mut self) -> Result<()> {
        self.send_request("shutdown", json!(null))?;
        self.send_notification("exit", json!(null))?;
        self.process.wait()?;
        Ok(())
    }
}

impl Drop for RustAnalyzerClient {
    fn drop(&mut self) {
        // Best effort shutdown
        let _ = self.send_request("shutdown", json!(null));
        let _ = self.send_notification("exit", json!(null));
        let _ = self.process.kill();
    }
}
