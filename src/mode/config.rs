use std::{
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
    str::FromStr,
};

use anyhow::anyhow;
use reqwest::{
    header::{self, HeaderMap},
    Url,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::api;

#[derive(Debug, Serialize, Deserialize)]
pub struct GlobalLunaticConfig {
    /// unique id for every installation of the cli tool
    /// used to identify "apps" during /cli/login calls
    pub cli_app_id: String,
    pub version: String,
    pub providers: Vec<Provider>,

    #[serde(skip)]
    config_file: Option<File>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub app_name: String,
    pub app_id: String,
    // in order to deploy an app there needs to be a compiled wasm file
    // which can come either from `main.rs`, a cargo `bin` or `example`
    // If neither a `bin` nor an `example` are provided it is assumed that
    // there's only a single app with the entrypoint at `main.rs`
    pub bin: Option<String>,
    pub example: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ProjectLunaticConfig {
    /// id of project on lunatic platform
    pub project_id: String,
    pub project_url: String,
    pub project_name: String,
    pub provider: String,

    // mapping for remote project
    pub remote: Vec<AppConfig>,

    #[serde(skip)]
    config_file: Option<File>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ConfigError {
    FileMissing(&'static str),
    TomlEncodingFailed,
    TomlDecodingFailed,
    FileWriteFailed(String),
    FileReadFailed(String),
}

impl FileBased for ProjectLunaticConfig {
    fn get_file(&mut self) -> File {
        // unwrapping should be safe because the struct is always constructed
        // with a file
        self.config_file
            .take()
            .expect("config_file should never be None, it's probably being called twice")
    }

    fn set_file(&mut self, config_file: File) {
        self.config_file = Some(config_file);
    }

    fn get_file_path() -> Result<PathBuf, ConfigError> {
        let mut current_dir = match std::env::current_dir() {
            Ok(dir) => dir,
            Err(_) => return Err(ConfigError::FileMissing("Failed to find lunatic.toml in working directory and parent directories. Are you sure you're in the correct directory?")),
        };

        loop {
            let candidate = current_dir.join("lunatic.toml");
            if candidate.exists() && candidate.is_file() {
                println!("FOUND lunatic.toml at '{candidate:?}'");
                return Ok(candidate);
            }

            if !current_dir.pop() {
                break;
            }
        }

        Err(ConfigError::FileMissing("Failed to find lunatic.toml in working directory and parent directories. Are you sure you're in the correct directory?"))
    }
}

impl FileBased for GlobalLunaticConfig {
    fn get_file(&mut self) -> File {
        // unwrapping should be safe because the struct is always constructed
        // with a file
        self.config_file
            .take()
            .expect("config_file should never be None, it's probably being called twice")
    }

    fn set_file(&mut self, config_file: File) {
        self.config_file = Some(config_file);
    }

    fn get_file_path() -> Result<PathBuf, ConfigError> {
        let home_path = dirs::home_dir().expect("failed to resolve home path for user");
        let lunatic_path = home_path.join(".lunatic");
        let config_path = lunatic_path.join("lunatic.toml");
        println!("GOT HOME PATH {home_path:?}");
        if let Err(e) = fs::read_dir(lunatic_path.clone()) {
            println!("[ConfigManager] GOT ERR {e:?}");
            fs::create_dir(lunatic_path).map_err(|_| {
                ConfigError::FileWriteFailed("failed to create ~/.lunatic directory".to_string())
            })?
        };
        Ok(config_path)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Provider {
    pub name: String,
    pub cookies: Vec<String>,
}

impl Provider {
    pub fn get_cookie_headers(&self) -> anyhow::Result<HeaderMap> {
        let mut headers = header::HeaderMap::new();
        for cookie in self.cookies.iter() {
            headers.insert(
                header::COOKIE,
                header::HeaderValue::from_str(cookie)
                    .map_err(|e| anyhow!("Failed to set lunatic platform cookie {e:?}"))?,
            );
        }
        Ok(headers)
    }

    pub fn get_url(&self) -> anyhow::Result<Url> {
        Url::from_str(&self.name)
            .map_err(|e| anyhow!("Failed to parse provider url '{}'. Error: {e:?}", self.name))
    }

    pub fn get_http_client(&self) -> anyhow::Result<reqwest::Client> {
        reqwest::ClientBuilder::new()
            .cookie_store(true)
            .default_headers(self.get_cookie_headers()?)
            .build()
            .map_err(|e| anyhow!("Failed to build reqwest client {e:?}"))
    }
}

impl Default for GlobalLunaticConfig {
    fn default() -> Self {
        Self {
            version: "0.1".to_string(),
            providers: vec![],
            cli_app_id: uuid::Uuid::new_v4().to_string(),

            config_file: None,
        }
    }
}

trait FileBased
where
    Self: Serialize + DeserializeOwned + Default,
{
    /// takes the file from the instance
    fn get_file(&mut self) -> File;

    fn set_file(&mut self, config_file: File);

    fn get_file_path() -> Result<PathBuf, ConfigError>;

    fn from_toml_file(path: PathBuf) -> (File, Self) {
        match fs::File::options()
            .append(true)
            .read(true)
            .write(true)
            .open(path.clone())
        {
            Ok(mut file) => {
                let mut buf = Vec::new();
                file.read_to_end(&mut buf)
                    .expect("failed to read lunatic.toml");
                println!("READ PROJECT TOML {buf:?}");
                let toml_str =
                    String::from_utf8(buf).expect("failed to read string from lunatic.toml");
                println!("READ PROJECT TOML AS STRING {toml_str}");
                let loaded_toml: Self = toml::from_str(&toml_str)
                    .or_else(|e| {
                        println!("FAILED TO DESERIALIZE toml {e:?}");
                        Ok::<Self, toml::de::Error>(Self::default())
                    })
                    .unwrap();
                (file, loaded_toml)
            }
            Err(_e) => {
                let mut file = fs::File::create(path).expect("failed to create new lunatic.toml");
                let initial_state = Self::default();
                let encoded = toml::to_vec(&initial_state).expect("Failed to encode toml");
                file.write_all(&encoded)
                    .expect("Failed to write toml to file");
                (file, initial_state)
            }
        }
    }

    fn flush_file(&mut self) -> Result<(), ConfigError> {
        let mut file = self.get_file();
        let encoded = toml::to_vec(self).map_err(|_| ConfigError::TomlEncodingFailed)?;
        file.write_all(&encoded).map_err(|_| {
            ConfigError::FileReadFailed(format!(
                "failed to encode config file '{:?}' as toml",
                Self::get_file_path()
            ))
        })
    }
}

#[derive(Debug)]
pub struct ConfigManager {
    pub global_config: GlobalLunaticConfig,
    // mapping of local project to platform Project/Apps
    pub project_config: ProjectLunaticConfig,
}

impl ConfigManager {
    pub fn new() -> Result<ConfigManager, ConfigError> {
        let global_config = ConfigManager::get_global_config()?;
        let project_config = ConfigManager::get_project_config()?;

        Ok(ConfigManager {
            global_config,
            project_config,
        })
    }

    pub async fn list_project_apps(&self) -> anyhow::Result<Vec<api::App>> {
        let provider = self.get_current_provider().await?;
        let client = provider.get_http_client()?;
        let list_apps_path = format!("/api/project/{}/apps", self.project_config.project_id);
        let list_apps_url = provider
            .get_url()?
            .join(&list_apps_path)
            .map_err(|e| anyhow!("Failed to join list_apps_url {e:?}"))?;

        let response = client
            .get(list_apps_url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed request to list apps in project {e:?}"))?;

        println!("Response from list apps {response:?}");

        response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse json from GET {list_apps_path} {e:?}"))
    }

    fn get_global_config() -> Result<GlobalLunaticConfig, ConfigError> {
        // make sure the config directory exists
        println!("START CONFIG MANAGER");
        let config_path = GlobalLunaticConfig::get_file_path()?;
        let (file, mut config) = GlobalLunaticConfig::from_toml_file(config_path);
        config.set_file(file);
        Ok(config)
    }

    fn get_project_config() -> Result<ProjectLunaticConfig, ConfigError> {
        let project_config_path = ProjectLunaticConfig::get_file_path()?;
        let (mut file, mut config) =
            ProjectLunaticConfig::from_toml_file(project_config_path.clone());
        let mut buf = vec![0u8; 4096];
        file.read_to_end(&mut buf).unwrap();
        println!(
            "FOUND PROJECT CONFIG {config:?} at '{project_config_path:?}' -> {}",
            String::from_utf8_lossy(&buf)
        );
        config.set_file(file);
        Ok(config)
    }

    pub async fn get_current_provider(&self) -> anyhow::Result<Provider> {
        println!(
            "LOOKING FOR CURRENT PROVIDER {} {:?}",
            self.project_config.provider, self.global_config.providers
        );
        let provider = self
            .global_config
            .providers
            .iter()
            .find(|provider| provider.name == self.project_config.provider)
            .ok_or_else(|| {
                anyhow!(
                    "Failed to find authenticated provider {}",
                    self.project_config.provider
                )
            })?;

        Ok(provider.clone())
    }

    pub fn set_version(&mut self, version: String) {
        self.global_config.version = version;
    }

    pub fn add_provider(&mut self, provider: Provider) {
        match self
            .global_config
            .providers
            .iter()
            .enumerate()
            .find(|(_, p)| p.name == provider.name)
        {
            Some((index, _)) => {
                self.global_config
                    .providers
                    .get_mut(index)
                    .expect("something went horribly wrong")
                    .cookies = provider.cookies;
            }
            None => self.global_config.providers.push(provider),
        }
    }

    pub fn get_app_id(&self) -> String {
        self.global_config.cli_app_id.clone()
    }

    pub fn get_version(&self) -> String {
        self.global_config.version.clone()
    }

    pub fn get_providers(&self) -> Vec<Provider> {
        self.global_config.providers.clone()
    }

    pub fn delete_providers(&mut self, name: String) -> Option<Provider> {
        if let Some((index, _)) = self
            .global_config
            .providers
            .iter()
            .enumerate()
            .find(|(_, provider)| provider.name == name)
        {
            return Some(self.global_config.providers.remove(index));
        }
        None
    }

    pub fn flush(&mut self) -> anyhow::Result<()> {
        // let toml_string = toml::to_string(&self.0)
        //     .expect("[config::flush] failed to encode LunaticConfig as toml");
        self.global_config
            .flush_file()
            .map_err(|e| anyhow!("Failed to flush ~/.lunatic/lunatic.toml config {e:?}"))
    }
}
