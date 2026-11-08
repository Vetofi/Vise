use super::Game;
use vise_client::{Result, control};

impl Game<'_> {
    pub(super) fn start_agent_control(&mut self) -> Result<()> {
        if let Some(path) = &self.options.control {
            let server = control::Server::start(path)
                .map_err(|e| format!("control socket {}: {e}", path.display()))?;
            log::info!("Vise: control socket {}", server.path().display());
            self.control = Some(server);
        }
        Ok(())
    }
}
