use async_std::prelude::FutureExt;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Row, Table};
use csv::Writer;
use figment::Figment;
use models::domain::Domain;
use modules::domains::diff_provider;
use modules::{
    cloudflare::CloudflareService, porkbun::PorkbunService, whois::whois, DomainService,
};
use state::{AppState, AppStateInner};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tracing::{info, warn};

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
