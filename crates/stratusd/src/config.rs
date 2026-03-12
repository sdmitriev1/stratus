use std::path::PathBuf;

pub struct Config {
    pub socket_path: PathBuf,
    pub data_dir: PathBuf,
}

impl Config {
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("stratus.db")
    }

    pub fn images_dir(&self) -> PathBuf {
        self.data_dir.join("images")
    }

    pub fn instances_dir(&self) -> PathBuf {
        self.data_dir.join("instances")
    }

    pub fn runtime_dir(&self) -> PathBuf {
        PathBuf::from("/run/stratus")
    }

    pub fn instance_runtime_dir(&self, name: &str) -> PathBuf {
        self.runtime_dir().join(name)
    }

    pub fn instance_data_dir(&self, name: &str) -> PathBuf {
        self.instances_dir().join(name)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/run/stratus/stratusd.sock"),
            data_dir: PathBuf::from("/var/lib/stratus"),
        }
    }
}
