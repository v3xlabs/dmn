pub mod cache;
pub mod cmd;
pub mod database;
pub mod models;
pub mod modules;
pub mod server;
pub mod state;
pub mod util;
pub mod web;

pub type Error = anyhow::Error;

#[async_std::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting dmn");

    cmd::handle_args().await?;

    Ok(())
}
