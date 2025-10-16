use anyhow::Result;

fn main() -> Result<()> {
    agentdev::web::run_blocking(agentdev::web::ServerOptions::from_env())
}
