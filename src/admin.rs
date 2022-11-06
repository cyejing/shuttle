use std::{net::SocketAddr, str::FromStr};

use anyhow::Ok;
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};
use serde_json::json;

use crate::{config::Admin, store::ServerStore};

pub async fn start_admin_server(admin_op: Option<Admin>, store: ServerStore) {
    if let Some(admin) = admin_op {
        let make_svc = make_service_fn(move |_conn| {
            let sc = store.clone();
            async move { Ok(service_fn(move |req| handle(req, sc.clone()))) }
        });

        info!("Start Admin addr : {}", &admin.addr);
        let addr = SocketAddr::from_str(&admin.addr).expect("addr str parse err");
        let server = Server::bind(&addr).serve(make_svc);

        if let Err(e) = server.await {
            eprintln!("server error: {}", e);
        }
    }
}

async fn handle(req: Request<Body>, store: ServerStore) -> anyhow::Result<Response<Body>> {
    let path = req.uri().path();
    match path {
        "/list" => list(store).await,
        _ => Ok(error(400, "unsupport_path", "unsupport path")),
    }
}

async fn list(store: ServerStore) -> anyhow::Result<Response<Body>> {
    let css = store.list_cmd_sender().await;
    let mut data = json!([]);
    for cs in css {
        data.as_array_mut().unwrap().push(json!({
            "hash": cs.hash,
            "status": "UP",
        }))
    }
    Ok(resp(serde_json::to_string(&data)?.as_str()))
}

fn error(status: u16, code: &str, msg: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::from(
            serde_json::json!({
                "status": status,
                "code": code,
                "msg": msg
            })
            .to_string(),
        ))
        .unwrap()
}

fn resp(data: &str) -> Response<Body> {
    Response::builder()
        .status(200)
        .body(Body::from(
            serde_json::json!({
                "status": 200,
                "data": data,
            })
            .to_string(),
        ))
        .unwrap()
}
