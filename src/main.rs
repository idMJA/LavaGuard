use lavaguard::cipher::CipherManager;
use lavaguard::config::load_config;
use lavaguard::routes::{create_router, AppState};
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lavaguard=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = Arc::new(load_config("config.toml"));
    let host = cfg.server.host.clone();
    let port = cfg.server.port;

    let cipher_mgr = Arc::new(CipherManager::new(Arc::clone(&cfg))?);
    let state = AppState {
        cipher: cipher_mgr,
        config: cfg,
    };

    let app = create_router(state);
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("LavaGuard running on http://{addr}");

    axum::serve(listener, app).await?;
    Ok(())
}
