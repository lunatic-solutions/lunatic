use std::{
    collections::HashMap,
    fs,
    io::{Read, Seek, Write},
    iter::FromIterator,
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

use super::api::{self, App, ProjectDetails, SaveApp};

#[derive(Debug, Serialize, Deserialize)]
pub struct GlobalLunaticConfig {
    /// unique id for every installation of the cli tool
    /// used to identify "apps" during /cli/login calls
    pub cli_app_id: String,
    pub version: String,
    pub providers: Vec<Provider>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct AppConfig {
    pub app_name: String,
    pub app_id: String,
    // in order to deploy an app there needs to be a compiled wasm file
    // which can come either from `main.rs`, a cargo `bin` or `example`
    // If neither a `bin` nor an `example` are provided it is assumed that
    // there's only a single app with the entrypoint at `main.rs`
    pub bin: Option<String>,
    pub example: Option<String>,
    // package means subcrate/workspace member
    pub package: Option<String>,
}

impl From<App> for AppConfig {
    fn from(new_app: App) -> Self {
        AppConfig {
            app_name: new_app.name,
            app_id: format!("{}", new_app.app_id),
            bin: None,
            example: None,
            package: None,
        }
    }
}

impl AppConfig {
    pub fn has_valid_mapping(&self) -> bool {
        matches!(
            (
                self.bin.as_ref(),
                self.example.as_ref(),
                self.package.as_ref(),
            ),
            (Some(_), None, None) | (None, Some(_), None) | (None, None, Some(_))
        )
    }

    pub fn get_binary_name(&self) -> String {
        format!(
            "{}.wasm",
            self.get_build_flags()
                .last()
                .expect("app should have binary name")
        )
    }

    pub fn get_build_flags(&self) -> Vec<&str> {
        let mut build_flags = vec![];
        if let Some(bin) = self.bin.as_deref() {
            build_flags.push("--bin");
            build_flags.push(bin);
            // let bins = bin.into_iter().flat_map(|b| ["--bin".to_owned(), b]);
            // vec!["build".to_owned()].into_iter().chain(bins).collect()
        } else if let Some(example) = self.example.as_deref() {
            build_flags.push("--example");
            build_flags.push(example);
        } else if let Some(package) = self.package.as_deref() {
            build_flags.push("--package");
            build_flags.push(package);
        }
        build_flags
    }
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
        let mut current_dir = match std::env::current_dir() {
            Ok(dir) => dir,
            Err(_) => return Err(ConfigError::FileMissing("Failed to find lunatic.toml in working directory and parent directories. Are you sure you're in the correct directory?")),
        };

        loop {
            let candidate = current_dir.join("lunatic.toml");
            if candidate.exists() && candidate.is_file() {
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
            providers: vec![],
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

    fn get_http_client(&self) -> anyhow::Result<(reqwest::Client, Provider)> {
        let provider = self.get_current_provider()?;
        let client = provider.get_http_client()?;
        Ok((client, provider))
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

    pub async fn update_project_apps(&mut self) -> anyhow::Result<()> {
        let new_apps = self
            .request_platform_skip_status::<Vec<api::App>, ()>(
                Method::GET,
                format!("/api/project/{}/apps", self.project_config.project_id),
                "list project apps",
                None,
                None,
            )
            .await?;

        let app_names: HashMap<String, bool> = HashMap::from_iter(
            self.project_config
                .remote
                .iter()
                .map(|app| (app.app_name.clone(), true)),
        );

        for new_app in new_apps
            .into_iter()
            .filter(|config_app| !app_names.contains_key(&config_app.name))
        {
            self.project_config.remote.push(new_app.into());
        }

        self.flush()?;

        Ok(())
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

    pub async fn create_new_app(&mut self, name: String) -> anyhow::Result<&mut AppConfig> {
        let new_app = self
            .request_platform_skip_status::<api::App, SaveApp>(
                Method::POST,
                format!("/api/project/{}/apps", self.project_config.project_id),
                "create new app",
                Some(SaveApp { name }),
                None,
            )
            .await?;

        self.project_config.remote.push(new_app.into());
        self.flush()?;

        Ok(self.project_config.remote.last_mut().unwrap())
    }

    pub async fn remove_app(&mut self, name: String) -> anyhow::Result<()> {
        self.project_config
            .remote
            .retain(|app| app.app_name != name);
        self.flush()?;
        Ok(())
    }

    pub async fn lookup_project(&self) -> anyhow::Result<ProjectDetails> {
        self.request_platform_skip_status(
            Method::GET,
            format!("{}", self.project_config.project_url),
            "create new app",
            None as Option<()>,
            None,
        )
        .await
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

    pub fn get_current_provider(&self) -> anyhow::Result<Provider> {
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

    pub fn find_app(&mut self, name: &str) -> Option<&mut AppConfig> {
        self.project_config
            .remote
            .iter_mut()
            .find(|a| a.app_name == name)
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

    pub fn logout_provider(&mut self, provider_to_delete: String) {
        self.global_config
            .providers
            .retain(|provider| provider.name != provider_to_delete);
    }

    pub fn get_app_id(&self) -> String {
        self.global_config.cli_app_id.clone()
    }

    // pub fn delete_providers(&mut self, name: String) -> Option<Provider> {
    //     if let Some((index, _)) = self
    //         .global_config
    //         .providers
    //         .iter()
    //         .enumerate()
    //         .find(|(_, provider)| provider.name == name)
    //     {
    //         return Some(self.global_config.providers.remove(index));
    //     }
    //     None
    // }

    pub fn flush(&mut self) -> anyhow::Result<()> {
        self.global_config
            .flush_file()
            .map_err(|e| anyhow!("Failed to flush ~/.lunatic/lunatic.toml config {e:?}"))?;

        self.project_config
            .flush_file()
            .map_err(|e| anyhow!("Failed to flush project lunatic.toml config {e:?}"))
    }
}
