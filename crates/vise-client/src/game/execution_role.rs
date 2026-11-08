
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExecutionRole {
    Client,
    IntegratedServer,
    DedicatedServer,
}

impl ExecutionRole {
    pub fn is_client(self) -> bool {
        self == Self::Client
    }

    pub fn starts_integrated_server(self, server_thread: bool) -> bool {
        self.is_client() && server_thread
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionRole::*;

    #[test]
    fn world_loading_keeps_in_frame_clients_and_server_roles_separate() {
        for (role, client, threaded) in [
            (Client, true, true),
            (IntegratedServer, false, false),
            (DedicatedServer, false, false),
        ] {
            assert_eq!(role.is_client(), client);
            assert_eq!(role.starts_integrated_server(true), threaded);
            assert!(!role.starts_integrated_server(false));
        }
    }
}
