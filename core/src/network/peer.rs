use std::hash::Hash;
use std::net::Ipv4Addr;
use std::string::ToString;

const DEFAULT_HOSTNAME: &str = "<empty>";

#[derive(Eq, Clone)]
pub struct Peer {
    host: String,
    port: u16,
    host_name: String,
    /// Stable per-install device id advertised by the peer. Empty when unknown
    /// (peer predates the field). NOT part of equality/hash — peers are still
    /// deduplicated by network endpoint `(host, port)`; this is carried for the
    /// host app to group a device's multiple endpoints as one.
    device_id: String,
}

impl Default for Peer {
    fn default() -> Self {
        Self {
            host: String::from("0.0.0.0"),
            port: 0,
            host_name: DEFAULT_HOSTNAME.to_string(),
            device_id: String::new(),
        }
    }
}

impl Hash for Peer {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.host.hash(state);
        self.port.hash(state);
    }
}

impl PartialEq for Peer {
    fn eq(&self, other: &Self) -> bool {
        self.host == other.host && self.port == other.port
    }

    fn ne(&self, other: &Self) -> bool {
        !self.eq(other)
    }
}

impl ToString for Peer {
    fn to_string(&self) -> String {
        format!("{}@{}:{}", &self.host_name, self.host, self.port,)
    }
}

impl Peer {
    pub fn from(socket_addr: &Ipv4Addr, port: u16, host_name: Option<&String>) -> Self {
        Self {
            host: socket_addr.to_string(),
            port,
            host_name: match host_name {
                Some(name) => name.clone(),
                None => DEFAULT_HOSTNAME.to_string(),
            },
            device_id: String::new(),
        }
    }

    pub fn new(host: &String, port: u16, host_name: Option<&String>) -> Self {
        Self {
            host: host.clone(),
            port,
            host_name: match host_name {
                Some(name) => name.clone(),
                None => DEFAULT_HOSTNAME.to_string(),
            },
            device_id: String::new(),
        }
    }

    pub fn host(&self) -> &String {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn host_name(&self) -> &String {
        &self.host_name
    }

    pub fn device_id(&self) -> &String {
        &self.device_id
    }

    /// Builder-style setter so existing `from`/`new` call sites stay unchanged;
    /// discovery sets the id after constructing the peer from a packet/record.
    pub fn with_device_id(mut self, device_id: impl Into<String>) -> Self {
        self.device_id = device_id.into();
        self
    }
}
