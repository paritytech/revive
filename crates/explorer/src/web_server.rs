//! HTTP server for the Compiler Explorer WebUI.
//!
//! This module implements the web server that serves the two-pane interface
//! and provides REST API endpoints for YUL source and RISC-V assembly data.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{Html, Json, Response},
    routing::get,
    Router,
};
use tower::ServiceBuilder;
use tower_http::{
    cors::CorsLayer,
    services::ServeDir,
};
use tokio::fs;
use anyhow::{anyhow, Result};

use crate::{
    api::{
        AnalysisMetadata, AnalysisResponse, AssemblyResponse, ErrorResponse, 
        LineQueryParams, MappingResponse, ToolVersions, YulSourceResponse
    },
    assembly_analyzer::AssemblyAnalyzer,
    dwarfdump,
    dwarfdump_analyzer::DwarfdumpAnalyzer,
    source_mapper::SourceMapper,
};

/// Shared application state for the web server.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Analysis data computed at startup
    pub analysis: Arc<AnalysisData>,
}

/// Complete analysis data for a shared object.
#[derive(Debug)]
pub struct AnalysisData {
    /// YUL source content and metadata
    pub yul_source: YulSourceResponse,
    /// RISC-V assembly instructions
    pub assembly: AssemblyResponse,
    /// Source-to-assembly mapping
    pub source_mapper: SourceMapper,
    /// Analysis metadata
    pub metadata: AnalysisMetadata,
}

/// Web server for the Compiler Explorer UI.
pub struct WebServer {
    shared_object_path: PathBuf,
    yul_source_path: PathBuf,
    dwarfdump_path: Option<PathBuf>,
    objdump_path: Option<PathBuf>,
}

impl WebServer {
    /// Creates a new web server instance.
    pub fn new(
        shared_object_path: PathBuf,
        yul_source_path: PathBuf,
        dwarfdump_path: Option<PathBuf>,
        objdump_path: Option<PathBuf>,
    ) -> Self {
        Self {
            shared_object_path,
            yul_source_path,
            dwarfdump_path,
            objdump_path,
        }
    }

    /// Starts the web server on the specified port.
    pub async fn serve(self, port: u16) -> Result<()> {
        println!("🔍 Analyzing shared object: {}", self.shared_object_path.display());
        
        // Perform analysis at startup as requested in the issue
        let analysis_data = self.analyze().await?;
        let app_state = AppState {
            analysis: Arc::new(analysis_data),
        };

        println!("✅ Analysis complete, starting web server...");

        // Create the router with API endpoints and static file serving
        let app = Router::new()
            .route("/", get(serve_index))
            .route("/api/analysis", get(get_analysis))
            .route("/api/yul", get(get_yul_source))
            .route("/api/assembly", get(get_assembly))
            .route("/api/mapping", get(get_mapping))
            .route("/api/assembly-for-line", get(get_assembly_for_line))
            .nest_service("/static", ServeDir::new("static"))
            .layer(
                ServiceBuilder::new()
                    .layer(CorsLayer::permissive())
            )
            .with_state(app_state);

        let bind_addr = format!("127.0.0.1:{}", port);
        let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
        
        println!("🚀 Revive Compiler Explorer running at: http://{}", bind_addr);
        println!("📖 Open the URL in your browser to view the YUL ↔ RISC-V interface");

        axum::serve(listener, app).await?;
        Ok(())
    }

    /// Performs the complete analysis as specified in issue #366.
    async fn analyze(&self) -> Result<AnalysisData> {
        // Step 1: Extract YUL source file path and debug info using dwarfdump
        let source_file = dwarfdump::source_file(&self.shared_object_path, &self.dwarfdump_path)?;
        let debug_lines = dwarfdump::debug_lines(&self.shared_object_path, &self.dwarfdump_path)?;

        // Step 2: Read YUL source content
        let yul_content = fs::read_to_string(&source_file).await
            .map_err(|e| anyhow!("Failed to read YUL source file {}: {}", source_file.display(), e))?;
        
        let yul_source = YulSourceResponse::new(
            yul_content,
            source_file.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        );

        // Step 3: Disassemble using objdump to get RISC-V instructions
        let assembly_analyzer = AssemblyAnalyzer::new(self.objdump_path.clone());
        let assembly_instructions = assembly_analyzer.disassemble(&self.shared_object_path)?;
        let assembly = AssemblyResponse::new(assembly_instructions.clone());

        // Step 4: Build source-to-assembly mapping using debug information
        let mut analyzer = DwarfdumpAnalyzer::new(&source_file, debug_lines.clone());
        analyzer.analyze()?;
        
        let mut source_mapper = SourceMapper::new();
        source_mapper.build_mapping(&debug_lines, &assembly_instructions)?;

        // Step 5: Create metadata
        let metadata = AnalysisMetadata {
            shared_object_path: self.shared_object_path.to_string_lossy().to_string(),
            yul_source_path: source_file.to_string_lossy().to_string(),
            analyzed_at: chrono::Utc::now().to_rfc3339(),
            tools: ToolVersions::detect(),
        };

        Ok(AnalysisData {
            yul_source,
            assembly,
            source_mapper,
            metadata,
        })
    }
}

// HTTP Handler Functions

/// Serves the main HTML page with the two-pane interface.
async fn serve_index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

/// Returns complete analysis data.
async fn get_analysis(State(state): State<AppState>) -> Json<AnalysisResponse> {
    let analysis = &state.analysis;
    Json(AnalysisResponse {
        yul_source: analysis.yul_source.clone(),
        assembly: analysis.assembly.clone(),
        mapping: MappingResponse::new(&analysis.source_mapper),
        metadata: analysis.metadata.clone(),
    })
}

/// Returns YUL source code.
async fn get_yul_source(State(state): State<AppState>) -> Json<YulSourceResponse> {
    Json(state.analysis.yul_source.clone())
}

/// Returns RISC-V assembly instructions.
async fn get_assembly(State(state): State<AppState>) -> Json<AssemblyResponse> {
    Json(state.analysis.assembly.clone())
}

/// Returns source-to-assembly mappings.
async fn get_mapping(State(state): State<AppState>) -> Json<MappingResponse> {
    Json(MappingResponse::new(&state.analysis.source_mapper))
}

/// Returns assembly addresses for a specific YUL line number.
async fn get_assembly_for_line(
    Query(params): Query<LineQueryParams>,
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, Response> {
    let addresses = state.analysis.source_mapper.get_assembly_for_line(params.line);
    let formatted_addresses: Vec<String> = addresses
        .into_iter()
        .map(|addr| format!("0x{:x}", addr))
        .collect();
    
    Ok(Json(formatted_addresses))
}

/// Helper function to create error responses.
async fn create_error_response(
    status: StatusCode,
    error: &str,
    message: &str,
) -> Response {
    let error_response = ErrorResponse::new(error, message);
    let json_body = serde_json::to_string(&error_response).unwrap_or_default();
    
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(json_body.into())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_web_server_creation() {
        let server = WebServer::new(
            PathBuf::from("/test/object.so"),
            PathBuf::from("/test/source.yul"),
            None,
            None,
        );
        
        assert_eq!(server.shared_object_path, PathBuf::from("/test/object.so"));
        assert_eq!(server.yul_source_path, PathBuf::from("/test/source.yul"));
    }

    #[tokio::test]
    async fn test_serve_index() {
        let response = serve_index().await;
        // The response should contain HTML
        assert!(response.0.contains("html") || response.0.contains("HTML"));
    }
}