use bitcoin::util::bip32::ExtendedPubKey;
use directories::BaseDirs;
use emulator_connect::connections::federated::FederatedEmulatorConnection;
use emulator_connect::connections::hd::HDOracleEmulatorConnection;
use emulator_connect::CTVEmulator;
use serde::*;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
#[derive(Serialize, Deserialize, Debug)]
pub struct EmulatorConfig {
    pub enabled: bool,
    pub emulators: Vec<(ExtendedPubKey, String)>,
    pub threshold: u8,
}
impl EmulatorConfig {
    pub fn get_emulator(&self) -> Result<Arc<dyn CTVEmulator>, Box<dyn std::error::Error>> {
        if self.emulators.len() < self.threshold as usize {
            Err(String::from("Too High Thresh"))?;
        } else if self.emulators.len() == 0 {
            Err(String::from("Too High Thresh"))?;
        }
        let _n_emulators = self.emulators.len();
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let secp = Arc::new(bitcoin::secp256k1::Secp256k1::new());
        let mut it =
            self.emulators
                .iter()
                .map(|(epk, host)| -> Result<_, Box<dyn std::error::Error>> {
                    Ok(HDOracleEmulatorConnection {
                        runtime: rt.clone(),
                        connection: Mutex::new(None),
                        reconnect: host.to_socket_addrs()?.next().unwrap(),
                        root: *epk,
                        secp: secp.clone(),
                    })
                });
        Ok(if self.emulators.len() == 1 {
            Arc::new(it.next().unwrap()?)
        } else {
            Arc::new(FederatedEmulatorConnection::new(
                it.map(|n| -> Result<_, Box<dyn std::error::Error>> {
                    let b: Arc<dyn CTVEmulator> = Arc::new(n?);
                    Ok(b)
                })
                .collect::<Result<Vec<_>, _>>()?,
                self.threshold,
            ))
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(into = "PathBuf")]
#[serde(from = "String")]
struct PathBufWrapped(PathBuf);
impl From<String> for PathBufWrapped {
    fn from(s: String) -> Self {
        PathBufWrapped(s.into())
    }
}
impl Into<PathBuf> for PathBufWrapped {
    fn into(self) -> PathBuf {
        self.0
    }
}
mod pathbuf {
    use serde::*;
    use std::path::PathBuf;
    pub fn serialize<S>(p: &PathBuf, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        s.serialize_str(p.to_str().unwrap())
    }
    pub fn deserialize<'de, D>(d: D) -> Result<PathBuf, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(String::deserialize(d)?.into())
    }
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(remote = "super::rpc::Auth")]
enum Auth {
    None,
    UserPass(String, String),
    CookieFile(#[serde(with = "pathbuf")] PathBuf),
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Node {
    pub url: String,
    #[serde(with = "Auth")]
    pub auth: super::rpc::Auth,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct NetworkConfig {
    pub active: bool,
    pub api_node: Node,
    pub emulator_nodes: Option<EmulatorConfig>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub plugin_map: Option<HashMap<String, WasmerCacheHash>>,
}

impl From<WasmerCacheHash> for [u8; 32] {
    fn from(x: WasmerCacheHash) -> Self {
        x.0.into()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(try_from = "String", into = "String")]
pub struct WasmerCacheHash([u8; 32]);

use bitcoin::hashes::hex::{FromHex, ToHex};
impl From<WasmerCacheHash> for String {
    fn from(x: WasmerCacheHash) -> Self {
        ToHex::to_hex(&x.0[..])
    }
}

impl TryFrom<String> for WasmerCacheHash {
    type Error = bitcoin::hashes::hex::Error;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        FromHex::from_hex(&s).map(WasmerCacheHash)
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(try_from = "ConfigVerifier")]
pub struct Config {
    pub active: NetworkConfig,
    pub network: bitcoin::network::constants::Network,
}

impl Config {
    pub async fn setup(
        matches: &clap::ArgMatches,
        typ: &str,
        org: &str,
        proj: &str,
    ) -> Result<Config, Box<dyn std::error::Error>> {
        if let Some(p) = matches.value_of("config") {
            Ok(serde_json::from_slice(&tokio::fs::read(p).await?[..])?)
        } else {
            let proj = directories::ProjectDirs::from(typ, org, proj)
                .expect("Failed to find config directory");
            let path = proj.config_dir();
            tokio::fs::create_dir_all(path).await?;
            let mut pb = path.to_path_buf();
            pb.push("config.json");
            if let Ok(txt) = tokio::fs::read(&pb).await {
                Ok(serde_json::from_slice(&txt[..])?)
            } else {
                let cfg = ConfigVerifier::default();
                tokio::fs::write(&pb, &serde_json::to_string_pretty(&cfg)?).await?;
                Ok(Config::try_from(cfg)?)
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ConfigVerifier {
    main: Option<NetworkConfig>,
    testnet: Option<NetworkConfig>,
    signet: Option<NetworkConfig>,
    regtest: Option<NetworkConfig>,
}
impl TryFrom<ConfigVerifier> for Config {
    type Error = ConfigError;

    fn try_from(
        cfg: ConfigVerifier,
    ) -> std::result::Result<Self, <Self as std::convert::TryFrom<ConfigVerifier>>::Error> {
        let network = cfg.get_network()?;
        Ok(Config {
            active: cfg.check()?,
            network,
        })
    }
}

impl ConfigVerifier {
    fn get_network(&self) -> Result<bitcoin::network::constants::Network, ConfigError> {
        match self.get_n() {
            1 => Err(ConfigError::NoActiveConfig),
            3 => Ok(bitcoin::network::constants::Network::Bitcoin),
            11 => Ok(bitcoin::network::constants::Network::Testnet),
            7 => Ok(bitcoin::network::constants::Network::Regtest),
            5 => {
                todo!()
            }
            _ => Err(ConfigError::TooManyActiveNetworks),
        }
    }
    fn get_n(&self) -> i32 {
        let v0 = self.main.as_ref().map(|c| 3 * c.active as i32).unwrap_or(1);
        let v1 = self
            .signet
            .as_ref()
            .map(|c| 5 * c.active as i32)
            .unwrap_or(1);
        let v2 = self
            .regtest
            .as_ref()
            .map(|c| 7 * c.active as i32)
            .unwrap_or(1);
        let v3 = self
            .testnet
            .as_ref()
            .map(|c| 11 * c.active as i32)
            .unwrap_or(1);
        v0 * v1 * v2 * v3
    }
    pub fn check(self) -> Result<NetworkConfig, ConfigError> {
        match self.get_n() {
            1 => Err(ConfigError::NoActiveConfig),
            3 => Ok(self.main.unwrap()),
            5 => Ok(self.signet.unwrap()),
            7 => Ok(self.regtest.unwrap()),
            11 => Ok(self.testnet.unwrap()),
            _ => Err(ConfigError::TooManyActiveNetworks),
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    TooManyActiveNetworks,
    NoActiveConfig,
}
use std::fmt;
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for ConfigError {}

impl std::default::Default for ConfigVerifier {
    fn default() -> Self {
        let mut b = BaseDirs::new()
            .expect("Could Not Determine a Base Directory")
            .home_dir()
            .to_path_buf();
        b.push(".bitcoin");
        b.push("regtest");
        b.push(".cookie");
        let regtest = NetworkConfig {
            active: true,
            api_node: Node{url: "http://127.0.0.1:18443".into(), auth: super::rpc::Auth::CookieFile(b.into())},
            emulator_nodes: Some(EmulatorConfig{
                enabled: true,
                threshold: 1u8,
                emulators: vec![(ExtendedPubKey::from_str("tpubD6NzVbkrYhZ4Wf398td3H8YhWBsXx9Sxa4W3cQWkNW3N3DHSNB2qtPoUMXrA6JNaPxodQfRpoZNE5tGM9iZ4xfUEFRJEJvfs8W5paUagYCE").unwrap(),
                    "ctv.d31373.org:8367".into())],
            }),
            plugin_map: None,
        };
        ConfigVerifier {
            main: None,
            testnet: None,
            signet: None,
            regtest: Some(regtest),
        }
    }
}