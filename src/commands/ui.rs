use anyhow::Result;

use agentdev::web::{ServerOptions, run_blocking};

pub fn handle_ui(port: u16) -> Result<()> {
    let options = ServerOptions::from_env().with_port(port);
    run_blocking(options)
}
