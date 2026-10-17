mod loader;
pub mod types;

pub use loader::read_config_file as load_config;
pub use types::Config;
