use std::path::PathBuf;

pub struct Config {
    pub socket_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/run/stratus/stratusd.sock"),
        }
    }
}
