use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub interface: String,
    pub interval_seconds: u64,
    pub discovery_port: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            interface: "vEthernet (Main)".into(),
            interval_seconds: 3,
            discovery_port: crate::protocol::DISCOVERY_PORT,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let Some(path) = settings_path() else {
            return Self::default();
        };
        let Ok(contents) = fs::read_to_string(path) else {
            return Self::default();
        };
        let mut settings = serde_json::from_str::<Self>(&contents).unwrap_or_default();
        if !(1..=300).contains(&settings.interval_seconds) {
            settings.interval_seconds = 3;
        }
        if settings.discovery_port == 0 {
            settings.discovery_port = crate::protocol::DISCOVERY_PORT;
        }
        settings
    }

    #[allow(dead_code)] // Used by the GUI binary; the service only reads shared settings.
    pub fn save(&self) -> Result<(), String> {
        let path = settings_path().ok_or("PROGRAMDATA is not available")?;
        let parent = path
            .parent()
            .ok_or("could not determine settings directory")?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create settings directory: {error}"))?;
        let contents = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("cannot encode settings: {error}"))?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, contents)
            .map_err(|error| format!("cannot write temporary settings: {error}"))?;
        fs::rename(&temporary, &path)
            .map_err(|error| format!("cannot replace settings file: {error}"))
    }
}

pub fn settings_path() -> Option<PathBuf> {
    std::env::var_os("PROGRAMDATA").map(|program_data| {
        PathBuf::from(program_data)
            .join("EdgeLink")
            .join("settings.json")
    })
}
