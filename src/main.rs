use my_media_sub::app::AppContext;
use my_media_sub::error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    my_media_sub::observability::init_tracing();
    tracing::info!("🦀 Starting my-media-sub Rust v2...");

    let config = my_media_sub::config::Config::load()?;
    tracing::info!("✅ Configuration loaded");
    tracing::info!("   Server: {}:{}", config.server.host, config.server.port);
    tracing::info!("   Data dir: {}", config.data_dir.display());

    let context = AppContext::new(&config).await?;
    context.start_background_services().await?;

    let app = my_media_sub::api::create_app(context);

    let addr = std::net::SocketAddr::from((
        config
            .server
            .host
            .parse::<std::net::IpAddr>()
            .unwrap_or_else(|_| std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0))),
        config.server.port,
    ));

    tracing::info!("🚀 Server starting on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("✅ Server listening on http://{}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
