use std::{collections::HashMap, sync::Arc};

use sha2::Digest as _;
use tracing::{debug, info};

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
    #[must_use]
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

    pub fn auth(&self, hash_str: &str) -> Option<String> {
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
        if let Some(hash) = &self.inner.password_hash {
            return (hash == hash_str).then(|| hash.clone());
        } else {
            info!("client password auth is not configured");
        }

        None
    }

    fn auth_userpass(&self, hash_str: &str) -> Option<String> {
        if let Some(users) = &self.inner.userpass {
            return users.get(hash_str).cloned();
        }

        info!("client userpass auth is not configured");
        None
    }
}

pub fn sha224(password: &str) -> String {
    let mut hasher = sha2::Sha224::new();
    hasher.update(password.as_bytes());
    let hash = hasher.finalize();
    let result = base16ct::lower::encode_string(&hash);
    debug!(length = result.len(), "sha224 generated");
    result
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn password_config() -> AuthConfig {
        AuthConfig {
            auth_type: AuthType::Password,
            password: Some("secret".to_string()),
            userpass: None,
            http: None,
            command: None,
        }
    }

    #[test]
    fn sha224_is_stable() {
        assert_eq!(
            sha224("secret"),
            "95c7fbca92ac5083afda62a564a3d014fc3b72c9140e3cb99ea6bf12"
        );
    }

    #[test]
    fn password_auth_returns_hash_on_match() {
        let config = password_config();
        let handler = AuthHandler::new(&config);
        let hash = sha224("secret");

        assert_eq!(handler.auth(&hash), Some(hash));
    }

    #[test]
    fn password_auth_rejects_mismatch() {
        let config = password_config();
        let handler = AuthHandler::new(&config);

        assert_eq!(handler.auth(&sha224("other")), None);
    }

    #[test]
    fn userpass_auth_returns_username() {
        let mut userpass = HashMap::new();
        userpass.insert("alice".to_string(), "wonderland".to_string());
        let config = AuthConfig {
            auth_type: AuthType::Userpass,
            password: None,
            userpass: Some(userpass),
            http: None,
            command: None,
        };
        let handler = AuthHandler::new(&config);

        assert_eq!(
            handler.auth(&sha224("alice:wonderland")),
            Some("alice".to_string())
        );
        assert_eq!(handler.auth(&sha224("alice:wrong")), None);
    }
}
