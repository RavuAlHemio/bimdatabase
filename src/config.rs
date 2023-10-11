use std::net::SocketAddr;

use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};


pub(crate) static CONFIG: OnceCell<Config> = OnceCell::new();


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Config {
    pub http: HttpConfig,
    pub db: DbConfig,
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
