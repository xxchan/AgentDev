use std::net::IpAddr;

use agentdev::web::{ServerOptions, run_blocking};
use anyhow::Result;

pub fn handle_ui(port: u16, host: Option<IpAddr>) -> Result<()> {
    let options = ServerOptions::from_env().with_port(port);
    let options = if let Some(host) = host {
        options.with_host(host)
    } else {
        options
    };
    run_blocking(options)
}
