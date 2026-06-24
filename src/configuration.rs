use sqlx::postgres::PgConnectOptions;

enum Environment {
    Local,
    Production
}

impl Environment {
    // 'static -> means for the ENTIRE LIFETIME of this program. We could return String, but that would mean allocating memory for this on the heap, which is not required. the "local.yaml" and "production.yaml" are string literals present in the binary.
    fn as_filename(&self) -> &'static str {
        match self {
            Environment::Local => "local.yaml",
            Environment::Production => "production.yaml"
        }
    }
}

// a fallible conversion from string into me.
// what if i set staging as the APP_ENVIRONMENT value in my .env or in the production dashboard? I can. It's a string. so, this is the idiomatic way to error that problem.
impl TryFrom<String> for Environment {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "local" => Ok(Environment::Local),
            "production" => Ok(Environment::Production),
            other => Err(format!("{other} is not a supported environment. Use either `local` or `production`."))
        }
    }
}

#[derive(serde::Deserialize)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub application_port: u16
}

#[derive(serde::Deserialize)]
pub struct DatabaseSettings {
   pub username: String,
   pub password: String,
   pub host: String,
   pub port: u16,
   pub database_name: String
}

impl DatabaseSettings {
    pub fn without_db(&self) -> PgConnectOptions {
        PgConnectOptions::new()
            .host(&self.host)
            .username(&self.username)
            .password(&self.password)
            .port(self.port)
            .database("postgres")
    }

    pub fn with_db(&self) -> PgConnectOptions {
        self.without_db().database(&self.database_name)
    }
}

pub fn get_configuration() -> Result<Settings, config::ConfigError> {

    let base_path = std::env::current_dir().expect("Failed to read current directory");
    let configuration_dir = base_path.join("configuration");

    let environment: Environment = std::env::var("APP_ENVIRONMENT")
    .unwrap_or_else(|_| "local".into())
    .try_into()
    .expect("Failed to parse APP_ENVIRONMENT");

    let settings = config::Config::builder()
    .add_source(config::File::from(configuration_dir.join("base.yaml")))
    .add_source(config::File::from(configuration_dir.join(environment.as_filename())))
    .add_source(config::Environment::with_prefix("APP")
    .prefix_separator("_")
    .separator("__"))
    .build()?;

    settings.try_deserialize::<Settings>()
}