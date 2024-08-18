use axum::{
    extract::{DefaultBodyLimit, Json, Multipart},
    http::StatusCode,
    routing::post,
    Router,
};
use eyre::Result;
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;
use xxfunc_db::{ModuleDatabase, ModuleState};

async fn deploy(
    mut multipart: Multipart,
    module_db: Arc<ModuleDatabase>,
) -> Result<String, StatusCode> {
    let mut file_name = String::new();

    while let Some(field) = multipart.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)? {
        let name = field.name().unwrap_or_default().to_string();
        if name == "module" {
            file_name = field
                .file_name()
                .map(|f| f.to_string())
                .unwrap_or_else(|| format!("{}.wasm", Uuid::new_v4()));
            info!("Received file: {}", file_name);
            let raw_data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;

            module_db
                .insert(&file_name, &raw_data)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            info!("Module '{}' inserted into database", file_name);
        }
    }

    if file_name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    Ok(file_name)
}

#[derive(Deserialize)]
struct StartInfo {
    module: String,
}

async fn start(
    Json(info): Json<StartInfo>,
    module_db: Arc<ModuleDatabase>,
) -> Result<String, StatusCode> {
    info!("Starting module: {}", info.module);
    module_db
        .set_state(&info.module, ModuleState::Started)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    info!("Module '{}' started successfully", info.module);
    Ok(info.module)
}

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing
    tracing_subscriber::fmt::init();
    let module_db = Arc::new(ModuleDatabase::open("module.db")?);
    info!("Module database initialized");

    let app = Router::new()
        .route(
            "/deploy",
            post({
                let module_db = Arc::clone(&module_db);
                move |multipart| deploy(multipart, module_db)
            }),
        )
        .route(
            "/start",
            post({
                let module_db = Arc::clone(&module_db);
                move |info| start(info, module_db)
            }),
        )
        .layer(DefaultBodyLimit::disable());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
    info!("Server started on port 3000");
    Ok(())
}
