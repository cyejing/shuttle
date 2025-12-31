use std::{collections::HashMap, sync::Arc};

use sha2::Digest as _;

use crate::config::{AuthConfig, AuthHttpConfig, AuthType};

#[derive(Clone)]
pub struct AuthHandler {
    inner: Arc<Inner>,
}

#[allow(dead_code)]
struct Inner {
    pub auth_type: AuthType,
    pub password_hash: Option<String>,
    pub userpass: Option<HashMap<String /*hash*/, String /*user*/>>,
    pub http: Option<AuthHttpConfig>,
    pub command: Option<String>,
}

impl AuthHandler {
    pub fn new(config: &AuthConfig) -> Self {
        let password_hash = config.password.as_ref().map(|p| sha224(p));
        let userpass = config.userpass.as_ref().map(|m| {
            m.iter()
                .map(|(k, v)| (sha224(&format!("{k}:{v}")), k.to_string()))
                .collect::<HashMap<String, String>>()
        });
        Self {
            inner: Arc::new(Inner {
                auth_type: config.auth_type.clone(),
                password_hash,
                userpass,
                http: config.http.clone(),
                command: config.command.clone(),
            }),
        }
    }

    pub async fn auth(&self, hash_str: &str) -> Option<String> {
        match self.inner.auth_type {
            AuthType::Password => self.auth_password(hash_str),
            AuthType::Userpass => self.auth_userpass(hash_str),
            AuthType::Http => {
                todo!()
            }
            AuthType::Command => {
                todo!()
            }
        }
    }

    fn auth_password(&self, hash_str: &str) -> Option<String> {
        match &self.inner.password_hash {
            Some(v) => {
                if v == hash_str {
                    Some(v.to_string())
                } else {
                    None
                }
            }
            None => {
                info!("client please config password");
                None
            }
        }
    }

    fn auth_userpass(&self, hash_str: &str) -> Option<String> {
        match &self.inner.userpass {
            Some(m) => {
                if let Some(u) = m.get(hash_str) {
                    Some(u.to_string())
                } else {
                    info!("client please config userpass");
                    None
                }
            }
            None => {
                info!("client please config userpass");
                None
            }
        }
    }
}

pub fn sha224(password: &str) -> String {
    let mut hasher = sha2::Sha224::new();
    hasher.update(password.as_bytes());
    let hash = hasher.finalize();
    let result = base16ct::lower::encode_string(&hash);
    log::debug!(
        "sha224({}) = {}, length = {}",
        password,
        result,
        result.len()
    );
    result
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    #[test]
    pub fn test_hashmap_map_value() {
        let map = HashMap::from([("a", 1), ("b", 2), ("c", 3)]);
        let new_map: HashMap<&str, i32> = map.iter().map(|(key, val)| (*key, val + 2)).collect();

        for val in new_map.values() {
            println!("{val}");
        }
    }
}
