pub mod anydrop_service;
pub mod context;
pub mod data_service;
pub mod discovery_service;
pub mod handler;
pub mod mdns_discovery;

pub type ShouldInterruptFunctionType = Box<dyn (Fn() -> bool) + Send + Sync>;
