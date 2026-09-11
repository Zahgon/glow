//! Layered configuration.
//!
//! Reimplements the `spf13/viper` behaviour glow relies on: a search path of
//! configuration directories, a YAML config file, `GLOW_`-prefixed environment
//! overrides, flags bound to keys, and in-code defaults — resolved in viper's
//! precedence order (flag → environment → file → default) with keys folded to
//! lower case.

use std::collections::BTreeMap;
use std::path::Path;

use crate::deps::cobra::{FlagSet, Value as FlagValue};

/// A configuration value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A boolean.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A string.
    Str(String),
}

impl Value {
    /// `cast.ToBool` semantics.
    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Str(s) => matches!(s.as_str(), "1" | "t" | "T" | "true" | "TRUE" | "True"),
        }
    }

    /// `cast.ToUint` semantics; a value that will not convert becomes zero.
    pub fn as_uint(&self) -> u64 {
        match self {
            Value::Bool(b) => u64::from(*b),
            Value::Int(i) => (*i).max(0) as u64,
            Value::Str(s) => s.trim().parse::<u64>().unwrap_or(0),
        }
    }

    /// `cast.ToString` semantics.
    pub fn as_string(&self) -> String {
        match self {
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Str(s) => s.clone(),
        }
    }
}

/// The configuration registry.
#[derive(Debug, Default)]
pub struct Viper {
    defaults: BTreeMap<String, Value>,
    config: BTreeMap<String, Value>,
    bindings: BTreeMap<String, String>,
    env_prefix: String,
    automatic_env: bool,
    config_paths: Vec<String>,
    config_name: String,
    config_type: String,
    config_file_used: String,
}

/// Why reading a configuration file failed.
#[derive(Debug)]
pub enum ReadError {
    /// No configuration file was found in any search path.
    NotFound,
    /// A file was found but could not be parsed.
    Parse(String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::NotFound => write!(f, "Config File \"glow\" Not Found"),
            ReadError::Parse(e) => write!(f, "{e}"),
        }
    }
}

/// Extensions viper recognises, in the order it probes them.
const SUPPORTED_EXTS: [&str; 4] = ["json", "toml", "yaml", "yml"];

impl Viper {
    /// An empty registry.
    pub fn new() -> Viper {
        Viper::default()
    }

    /// Appends a directory to the configuration search path.
    pub fn add_config_path(&mut self, path: &str) {
        if !path.is_empty() {
            self.config_paths.push(path.to_string());
        }
    }

    /// Sets the base name of the configuration file.
    pub fn set_config_name(&mut self, name: &str) {
        self.config_name = name.to_string();
    }

    /// Sets the expected configuration format.
    pub fn set_config_type(&mut self, ty: &str) {
        self.config_type = ty.to_string();
    }

    /// Sets the prefix used for environment lookups.
    pub fn set_env_prefix(&mut self, prefix: &str) {
        self.env_prefix = prefix.to_string();
    }

    /// Enables environment lookups for every key.
    pub fn automatic_env(&mut self) {
        self.automatic_env = true;
    }

    /// Sets a default for `key`.
    pub fn set_default(&mut self, key: &str, value: Value) {
        self.defaults.insert(key.to_lowercase(), value);
    }

    /// Binds a configuration key to a flag name.
    pub fn bind_pflag(&mut self, key: &str, flag: &str) {
        self.bindings.insert(key.to_lowercase(), flag.to_string());
    }

    /// The path of the configuration file that was read, or an empty string.
    pub fn config_file_used(&self) -> &str {
        &self.config_file_used
    }

    /// Searches the configured paths and loads the first configuration found.
    pub fn read_in_config(&mut self) -> Result<(), ReadError> {
        let file = match self.find_config_file() {
            Some(f) => f,
            None => return Err(ReadError::NotFound),
        };
        let text = std::fs::read_to_string(&file)
            .map_err(|e| ReadError::Parse(format!("{}: {e}", file)))?;
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&text).map_err(|e| ReadError::Parse(e.to_string()))?;
        if let serde_yaml::Value::Mapping(map) = parsed {
            for (k, v) in map {
                let key = match k {
                    serde_yaml::Value::String(s) => s.to_lowercase(),
                    other => format!("{other:?}").to_lowercase(),
                };
                if let Some(value) = yaml_to_value(&v) {
                    self.config.insert(key, value);
                }
            }
        }
        self.config_file_used = file;
        Ok(())
    }

    fn find_config_file(&self) -> Option<String> {
        for dir in &self.config_paths {
            for ext in SUPPORTED_EXTS {
                let candidate =
                    format!("{}/{}.{}", dir.trim_end_matches('/'), self.config_name, ext);
                if Path::new(&candidate).is_file() {
                    return Some(candidate);
                }
            }
            if !self.config_type.is_empty() {
                let candidate = format!("{}/{}", dir.trim_end_matches('/'), self.config_name);
                if Path::new(&candidate).is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    /// Resolves `key` in viper's precedence order.
    pub fn get(&self, key: &str, flags: &FlagSet) -> Option<Value> {
        let key = key.to_lowercase();

        if let Some(flag_name) = self.bindings.get(&key) {
            if let Some(flag) = flags.lookup(flag_name) {
                if flag.changed {
                    return Some(flag_value(&flag.value));
                }
            }
        }
        if self.automatic_env {
            let env_key = if self.env_prefix.is_empty() {
                key.to_uppercase()
            } else {
                format!("{}_{}", self.env_prefix.to_uppercase(), key.to_uppercase())
            };
            if let Ok(v) = std::env::var(&env_key) {
                if !v.is_empty() {
                    return Some(Value::Str(v));
                }
            }
        }
        if let Some(v) = self.config.get(&key) {
            return Some(v.clone());
        }
        if let Some(v) = self.defaults.get(&key) {
            return Some(v.clone());
        }
        // Last resort: the flag's own default.
        if let Some(flag_name) = self.bindings.get(&key) {
            if let Some(flag) = flags.lookup(flag_name) {
                return Some(flag_value(&flag.default));
            }
        }
        None
    }

    /// `key` as a boolean.
    pub fn get_bool(&self, key: &str, flags: &FlagSet) -> bool {
        self.get(key, flags).map(|v| v.as_bool()).unwrap_or(false)
    }

    /// `key` as an unsigned integer.
    pub fn get_uint(&self, key: &str, flags: &FlagSet) -> u64 {
        self.get(key, flags).map(|v| v.as_uint()).unwrap_or(0)
    }

    /// `key` as a string.
    pub fn get_string(&self, key: &str, flags: &FlagSet) -> String {
        self.get(key, flags)
            .map(|v| v.as_string())
            .unwrap_or_default()
    }
}

fn flag_value(v: &FlagValue) -> Value {
    match v {
        FlagValue::Bool(b) => Value::Bool(*b),
        FlagValue::Str(s) => Value::Str(s.clone()),
        FlagValue::Uint(u) => Value::Int(*u as i64),
    }
}

fn yaml_to_value(v: &serde_yaml::Value) -> Option<Value> {
    match v {
        serde_yaml::Value::Bool(b) => Some(Value::Bool(*b)),
        serde_yaml::Value::Number(n) => n.as_i64().map(Value::Int),
        serde_yaml::Value::String(s) => Some(Value::Str(s.clone())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deps::cobra::Flag;

    fn flags() -> FlagSet {
        let mut set = FlagSet::new();
        set.add(Flag::string("style", "s", "auto", "style"));
        set.add(Flag::uint("width", "w", 0, "width"));
        set.add(Flag::bool("all", "a", false, "all"));
        set
    }

    #[test]
    fn defaults_are_used_when_nothing_else_matches() {
        let mut v = Viper::new();
        v.set_default("style", Value::Str("auto".into()));
        assert_eq!(v.get_string("style", &flags()), "auto");
    }

    #[test]
    fn keys_are_case_insensitive() {
        let mut v = Viper::new();
        v.set_default("preserveNewLines", Value::Bool(true));
        assert!(v.get_bool("preservenewlines", &flags()));
        assert!(v.get_bool("PreserveNewLines", &flags()));
    }

    #[test]
    fn a_changed_flag_outranks_config_and_default() {
        let mut v = Viper::new();
        v.set_default("style", Value::Str("auto".into()));
        v.config.insert("style".into(), Value::Str("dark".into()));
        v.bind_pflag("style", "style");
        let mut f = flags();
        assert_eq!(v.get_string("style", &f), "dark");
        // Simulate the flag being set on the command line.
        f = {
            let mut set = FlagSet::new();
            let mut flag = Flag::string("style", "s", "auto", "style");
            flag.value = FlagValue::Str("light".into());
            flag.changed = true;
            set.add(flag);
            set
        };
        assert_eq!(v.get_string("style", &f), "light");
    }

    #[test]
    fn environment_outranks_config() {
        let mut v = Viper::new();
        v.set_env_prefix("glow");
        v.automatic_env();
        v.config.insert("style".into(), Value::Str("dark".into()));
        std::env::set_var("GLOW_STYLE", "pink");
        assert_eq!(v.get_string("style", &flags()), "pink");
        std::env::remove_var("GLOW_STYLE");
        assert_eq!(v.get_string("style", &flags()), "dark");
    }

    #[test]
    fn reads_a_yaml_config_from_the_search_path() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            dir.path().join("glow.yml"),
            "style: \"light\"\nwidth: 90\nall: true\n",
        )
        .expect("write config");
        let mut v = Viper::new();
        v.set_config_name("glow");
        v.set_config_type("yaml");
        v.add_config_path(dir.path().to_str().expect("utf-8 path"));
        v.read_in_config().expect("config reads");
        assert!(v.config_file_used().ends_with("glow.yml"));
        assert_eq!(v.get_string("style", &flags()), "light");
        assert_eq!(v.get_uint("width", &flags()), 90);
        assert!(v.get_bool("all", &flags()));
    }

    #[test]
    fn missing_config_is_reported_as_not_found() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut v = Viper::new();
        v.set_config_name("glow");
        v.set_config_type("yaml");
        v.add_config_path(dir.path().to_str().expect("utf-8 path"));
        assert!(matches!(v.read_in_config(), Err(ReadError::NotFound)));
        assert_eq!(v.config_file_used(), "");
    }

    #[test]
    fn cast_rules_match_spf13_cast() {
        assert!(Value::Str("true".into()).as_bool());
        assert!(!Value::Str("yes".into()).as_bool());
        assert!(Value::Int(3).as_bool());
        assert_eq!(Value::Str("42".into()).as_uint(), 42);
        assert_eq!(Value::Str("nope".into()).as_uint(), 0);
        assert_eq!(Value::Bool(true).as_string(), "true");
    }
}
