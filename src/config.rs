use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};


pub(crate) static CONFIG: OnceLock<Config> = OnceLock::new();


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Config {
    pub http: HttpConfig,
    pub db: DbConfig,
    #[serde(default = "Config::default_vehicles_per_page")] pub vehicles_per_page: i64,
    #[serde(default)] pub value_sets: ValueSetConfig,
}
impl Config {
    fn default_vehicles_per_page() -> i64 { 20 }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct HttpConfig {
    pub listen_socket_addr: SocketAddr,
    pub base_path: String,
    #[serde(default)] pub static_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct DbConfig {
    pub username: String,
    pub password: String,
    pub hostname: String,
    pub db_name: String,
    #[serde(default = "DbConfig::default_port")] pub port: u16,
}
impl DbConfig {
    fn default_port() -> u16 { 5432 }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ValueSetConfig {
    #[serde(default)] pub vehicle_classes: BTreeSet<String>,
    #[serde(default)] pub power_sources: BTreeSet<String>,
}
