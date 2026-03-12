// Phase A: Hardcoded configuration values.
// In later phases, this will be replaced with a proper configuration loading mechanism.

pub const LISTEN_ADDR: &str = "127.0.0.1:8080";

pub fn backends() -> Vec<String> {
    vec![
        "127.0.0.1:9000".to_string(),
        "127.0.0.1:9001".to_string(),
    ]
}