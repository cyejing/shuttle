use shuttle::{
    auth::sha224,
    config::{load_client_config, load_server_config},
};
use std::path::PathBuf;

#[test]
fn example_client_proxy_config_is_loadable_and_hashed() {
    let config = load_client_config(Some(PathBuf::from("tests/examples/client-proxy.yaml")));

    assert_eq!(config.logs, PathBuf::from("logs"));

    assert_eq!(config.get_proxy_auth_hash(), sha224("sQtfRnfhcNoZYZh1wY9u"));
    assert!(config.hole.is_none());
}

#[test]
fn example_client_rathole_config_is_loadable_and_hashed() {
    let config = load_client_config(Some(PathBuf::from("tests/examples/client-rathole.yaml")));

    assert_eq!(config.logs, PathBuf::from("logs"));
    assert!(config.proxy.is_none());
    assert_eq!(config.get_hole_auth_hash(), sha224("58JCEmvcBkRAk1XkK1iH"));
    assert_eq!(config.get_holes().len(), 1);
    assert_eq!(config.get_holes()[0].name, "test");
}

#[test]
fn example_server_config_is_loadable_and_hashes_rathole_passwords() {
    let config = load_server_config(Some(PathBuf::from("tests/examples/server.yaml")));
    let rathole = config.rathole.expect("rathole config should exist");

    assert_eq!(config.logs, PathBuf::from("logs"));
    assert_eq!(config.listen, "127.0.0.1:4982");
    assert_eq!(
        config.auth.password.as_deref(),
        Some("sQtfRnfhcNoZYZh1wY9u")
    );
    assert_eq!(rathole.password_hash, vec![sha224("58JCEmvcBkRAk1XkK1iH")]);
}
