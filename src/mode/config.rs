use std::{
    fs,
    io::{Read, Seek, Write},
    path::PathBuf,
    str::FromStr,
};

use anyhow::anyhow;
use log::debug;
use reqwest::{
    header::{self, HeaderMap},
    multipart::{self, Form},
    Method, StatusCode, Url,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct GlobalLunaticConfig {
    /// unique id for every installation of the cli tool
    /// used to identify "apps" during /cli/login calls
    pub cli_app_id: String,
    pub version: String,
    pub provider: Option<Provider>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ProjectLunaticConfig {
    pub project_id: i64,
    pub project_name: String,
    pub domains: Vec<String>,
    pub app_id: i64,
    pub env_id: i64,
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
    fn get_file_path() -> Result<PathBuf, ConfigError> {
        let current_dir = match std::env::current_dir() {
            Ok(dir) => dir,
            Err(_) => return Err(ConfigError::FileMissing("Failed to find lunatic.toml in working directory and parent directories. Are you sure you're in the correct directory?")),
        };
        Ok(current_dir.join("lunatic.toml"))
    }
}

impl FileBased for GlobalLunaticConfig {
    fn get_file_path() -> Result<PathBuf, ConfigError> {
        let home_path = dirs::home_dir().expect("failed to resolve home path for user");
        let lunatic_path = home_path.join(".lunatic");
        let config_path = lunatic_path.join("lunatic.toml");
        if let Err(e) = fs::read_dir(lunatic_path.clone()) {
            fs::create_dir(lunatic_path).map_err(|_| {
                ConfigError::FileWriteFailed(format!(
                    "failed to create ~/.lunatic directory. Error: {e:?}"
                ))
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
            provider: None,
            cli_app_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

pub(crate) trait FileBased
where
    Self: Serialize + DeserializeOwned + Default,
{
    fn get_file_path() -> Result<PathBuf, ConfigError>;

    fn from_toml_file(path: PathBuf) -> Self {
        match fs::File::options()
            .create(true)
            .read(true)
            .write(true)
            .open(path.clone())
        {
            Ok(mut file) => {
                let mut buf = Vec::new();
                file.read_to_end(&mut buf)
                    .expect("failed to read lunatic.toml");
                let toml_str =
                    String::from_utf8(buf).expect("failed to read string from lunatic.toml");
                let loaded_toml: Self = toml::from_str(&toml_str)
                    .or_else(|_e| Ok::<Self, toml::de::Error>(Self::default()))
                    .unwrap();
                loaded_toml
            }
            Err(_e) => {
                let mut file = fs::File::create(path).expect("failed to create new lunatic.toml");
                let initial_state = Self::default();
                let encoded = toml::to_vec(&initial_state).expect("Failed to encode toml");
                file.write_all(&encoded)
                    .expect("Failed to write toml to file");
                initial_state
            }
        }
    }

    fn flush_file(&mut self) -> Result<(), ConfigError> {
        let file_path = Self::get_file_path()?;
        let mut file = fs::File::options()
            .truncate(true)
            .create(true)
            .read(true)
            .write(true)
            .open(file_path)
            .map_err(|_e| ConfigError::FileMissing("-"))?;
        let encoded = toml::to_vec(self).map_err(|_| ConfigError::TomlEncodingFailed)?;
        file.rewind()
            .map_err(|e| ConfigError::FileWriteFailed(e.to_string()))?;
        file.write_all(&encoded).map_err(|_| {
            ConfigError::FileReadFailed(format!(
                "failed to encode config file '{:?}' as toml",
                Self::get_file_path()
            ))
        })?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ConfigManager {
    pub global_config: GlobalLunaticConfig,
    // mapping of local project to platform Project/Apps
    pub project_config: Option<ProjectLunaticConfig>,
}

impl ConfigManager {
    pub fn new() -> Result<ConfigManager, ConfigError> {
        let global_config = ConfigManager::get_global_config()?;
        let project_config = ConfigManager::get_project_config().ok();

        Ok(ConfigManager {
            global_config,
            project_config,
        })
    }

    pub fn get_http_client(&self) -> anyhow::Result<(reqwest::Client, Provider)> {
        match self.global_config.provider.clone() {
            Some(provider) => {
                let client = provider.get_http_client()?;
                Ok((client, provider))
            }
            None => Err(anyhow!("First login by calling `lunatic login`")),
        }
    }

    async fn request_platform_skip_status<T: DeserializeOwned, I: Serialize>(
        &self,
        method: Method,
        path: String,
        description: &str,
        body: Option<I>,
        form_body: Option<Form>,
    ) -> anyhow::Result<T> {
        let (_, response) = self
            .request_platform(method, path, description, body, form_body)
            .await?;
        Ok(response)
    }

    // quality of life function that makes all calls to platform
    async fn request_platform<T: DeserializeOwned, I: Serialize>(
        &self,
        method: Method,
        path: String,
        description: &str,
        body: Option<I>,
        form_body: Option<Form>,
    ) -> anyhow::Result<(StatusCode, T)> {
        let (client, provider) = self.get_http_client()?;
        let full_url = provider
            .get_url()?
            .join(&path)
            .map_err(|e| anyhow!("Failed to join url {e:?}"))?;

        let mut builder = client.request(method, full_url.clone());

        builder = if let Some(b) = &body {
            builder.json(&b)
        } else if let Some(form) = form_body {
            builder.multipart(form)
        } else {
            builder
        };

        let response = builder
            .send()
            .await
            .map_err(|e| anyhow!("Failed request to {description} in project {e:?}"))?;

        debug!("Response from '{description}' {response:?}");

        Ok((
            response.status(),
            response
                .json()
                .await
                .map_err(|e| anyhow!("Failed to parse json from GET {full_url} {e:?}"))?,
        ))
    }

    pub fn project_config(&self) -> anyhow::Result<&ProjectLunaticConfig> {
        self.project_config
            .as_ref()
            .ok_or_else(|| anyhow!("Project `lunatic.toml` not found in current directory."))
    }

    pub fn project_config_mut(&mut self) -> anyhow::Result<&mut ProjectLunaticConfig> {
        self.project_config
            .as_mut()
            .ok_or_else(|| anyhow!("Project `lunatic.toml` not found in current directory."))
    }

    pub fn init_project(&mut self, project_config: ProjectLunaticConfig) {
        self.project_config = Some(project_config);
    }

    pub async fn upload_artefact_for_app(
        &mut self,
        app_id: &str,
        artefact: Vec<u8>,
        filename: String,
    ) -> anyhow::Result<i64> {
        let form = multipart::Form::new().part(
            "file",
            multipart::Part::stream(artefact).file_name(filename),
        );
        let (status, new_version) = self
            .request_platform::<serde_json::Value, ()>(
                Method::POST,
                format!("/api/apps/{app_id}/versions"),
                "upload wasm",
                None,
                Some(form),
            )
            .await?;

        if let (Some(serde_json::Value::Number(version_id)), true) =
            (new_version.get("app_version_id"), status.is_success())
        {
            version_id.as_i64().ok_or_else(|| {
                anyhow!("Failed to cast app_version_id to i64 from response {new_version:?}")
            })
        } else {
            Err(anyhow!(
                "Failed to get app_version_id from response {new_version:?}"
            ))
        }
    }

    fn get_global_config() -> Result<GlobalLunaticConfig, ConfigError> {
        // make sure the config directory exists
        let config_path = GlobalLunaticConfig::get_file_path()?;
        Ok(GlobalLunaticConfig::from_toml_file(config_path))
    }

    fn get_project_config() -> Result<ProjectLunaticConfig, ConfigError> {
        let project_config_path = ProjectLunaticConfig::get_file_path()?;
        Ok(ProjectLunaticConfig::from_toml_file(project_config_path))
    }

    pub fn login(&mut self, provider: Provider) {
        self.global_config.provider = Some(provider);
    }

    pub fn logout(&mut self) {
        self.global_config.provider = None;
    }

    pub fn get_app_id(&self) -> String {
        self.global_config.cli_app_id.clone()
    }

    pub fn flush(&mut self) -> anyhow::Result<()> {
        self.global_config
            .flush_file()
            .map_err(|e| anyhow!("Failed to flush ~/.lunatic/lunatic.toml config {e:?}"))?;

        match self.project_config.as_mut() {
            Some(project_config) => project_config
                .flush_file()
                .map_err(|e| anyhow!("Failed to flush project lunatic.toml config {e:?}")),
            None => Ok(()),
        }
    }
}
