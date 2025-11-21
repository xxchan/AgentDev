use std::net::IpAddr;

use agentdev::web::{ServerOptions, run_blocking};
use anyhow::Result;

pub fn handle_ui(port: u16, host: Option<IpAddr>, headless: bool) -> Result<()> {
    let mut options = ServerOptions::from_env().with_port(port);
    if let Some(host) = host {
        options = options.with_host(host);
    }

    // Headless mode overrides any environment variable setting
    if headless {
        options = options.with_auto_open(false);
    } else if std::env::var("AGENTDEV_AUTO_OPEN_BROWSER").is_err() {
        options = options.with_auto_open(true);
    }

    run_blocking(options)
}
