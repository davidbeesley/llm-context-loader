mod logging;
mod config;

use anyhow::Result;
use log::{debug, info};
use rmcp::{tool, ServerHandler, ServiceExt, schemars};
use rmcp::model::{ServerCapabilities, ServerInfo};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::config::Config;

#[derive(Debug, Clone)]
struct LlmContextLoader {
    config: Arc<RwLock<Config>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FileSummaryRequest {
    #[schemars(description = "Path to the file to summarize")]
    file_path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ModuleSummaryRequest {
    #[schemars(description = "Path to the module or directory")]
    module_path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchSymbolsRequest {
    #[schemars(description = "Symbol name or pattern to search for")]
    query: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SymbolInfoRequest {
    #[schemars(description = "Name of the symbol")]
    symbol_name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CrateDocsRequest {
    #[schemars(description = "Name of the crate")]
    crate_name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ProjectStructureRequest {
    #[schemars(description = "Root path to start listing from (optional)")]
    root_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SetPromptRequest {
    #[schemars(description = "The summarization prompt template")]
    prompt: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RegenerateSummaryRequest {
    #[schemars(description = "Path to the file or module to regenerate summary for")]
    target: String,
}

#[tool(tool_box)]
impl LlmContextLoader {
    #[tool(description = "Get a summary of a file's contents")]
    async fn get_file_summary(&self, #[tool(aggr)] req: FileSummaryRequest) -> String {
        debug!("get_file_summary called for: {}", req.file_path);
        json!({
            "summary": format!("Summary for file: {}", req.file_path),
            "status": "success"
        }).to_string()
    }

    #[tool(description = "Get a summary of a module or directory")]
    async fn get_module_summary(&self, #[tool(aggr)] req: ModuleSummaryRequest) -> String {
        debug!("get_module_summary called for: {}", req.module_path);
        json!({
            "summary": format!("Summary for module: {}", req.module_path),
            "status": "success"
        }).to_string()
    }

    #[tool(description = "Search for symbols in the codebase")]
    async fn search_symbols(&self, #[tool(aggr)] req: SearchSymbolsRequest) -> String {
        debug!("search_symbols called with query: {}", req.query);
        json!({
            "symbols": [],
            "query": req.query,
            "status": "success"
        }).to_string()
    }

    #[tool(description = "Get detailed information about a specific symbol")]
    async fn get_symbol_info(&self, #[tool(aggr)] req: SymbolInfoRequest) -> String {
        debug!("get_symbol_info called for: {}", req.symbol_name);
        json!({
            "info": format!("Information for symbol: {}", req.symbol_name),
            "status": "success"
        }).to_string()
    }

    #[tool(description = "Get documentation for a Rust crate")]
    async fn get_crate_docs(&self, #[tool(aggr)] req: CrateDocsRequest) -> String {
        debug!("get_crate_docs called for: {}", req.crate_name);
        json!({
            "docs": format!("Documentation for crate: {}", req.crate_name),
            "status": "success"
        }).to_string()
    }

    #[tool(description = "List the project structure")]
    async fn list_project_structure(&self, #[tool(aggr)] req: ProjectStructureRequest) -> String {
        let root_path = req.root_path.as_deref().unwrap_or(".");
        debug!("list_project_structure called for: {}", root_path);
        json!({
            "structure": [],
            "root": root_path,
            "status": "success"
        }).to_string()
    }

    #[tool(description = "Get project dependencies")]
    async fn get_dependencies(&self) -> String {
        debug!("get_dependencies called");
        json!({
            "dependencies": [],
            "status": "success"
        }).to_string()
    }

    #[tool(description = "Get the current summarization prompt")]
    async fn get_summarization_prompt(&self) -> String {
        debug!("get_summarization_prompt called");
        let config = self.config.read().await;
        json!({
            "prompt": config.default_summarization_prompt,
            "status": "success"
        }).to_string()
    }

    #[tool(description = "Set a custom summarization prompt")]
    async fn set_summarization_prompt(&self, #[tool(aggr)] req: SetPromptRequest) -> String {
        debug!("set_summarization_prompt called with: {}", req.prompt);
        let mut config = self.config.write().await;
        config.default_summarization_prompt = req.prompt.clone();
        json!({
            "success": true,
            "prompt": req.prompt,
            "status": "success"
        }).to_string()
    }

    #[tool(description = "Reset the summarization prompt to default")]
    async fn reset_summarization_prompt(&self) -> String {
        debug!("reset_summarization_prompt called");
        let mut config = self.config.write().await;
        let default_prompt = Config::default().default_summarization_prompt;
        config.default_summarization_prompt = default_prompt.clone();
        json!({
            "success": true,
            "prompt": default_prompt,
            "status": "success"
        }).to_string()
    }

    #[tool(description = "Regenerate a summary for a file or module")]
    async fn regenerate_summary(&self, #[tool(aggr)] req: RegenerateSummaryRequest) -> String {
        debug!("regenerate_summary called for: {}", req.target);
        json!({
            "summary": format!("Regenerated summary for: {}", req.target),
            "status": "success"
        }).to_string()
    }
}

#[tool(tool_box)]
impl ServerHandler for LlmContextLoader {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: rmcp::model::Implementation {
                name: "llm-context-loader".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: Some("LLM Context Loader - provides tools for loading and analyzing code context".into()),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging - MCP requires minimal logging
    logging::debug();
    
    debug!("Starting LLM Context Loader MCP Server");
    
    // Load configuration from environment
    let config = Config::from_env()?;
    info!("Loaded configuration: max_file_size={}", config.max_file_size);

    // Create the service
    let service = LlmContextLoader {
        config: Arc::new(RwLock::new(config)),
    };

    // Create stdio transport
    let transport = rmcp::transport::stdio();

    info!("MCP server listening on stdio");
    
    // Serve the service with the transport
    let server = service.serve(transport).await?;
    
    // Wait for the server to complete
    let quit_reason = server.waiting().await?;
    info!("Server shut down: {:?}", quit_reason);
    
    Ok(())
}