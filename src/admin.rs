use std::{net::SocketAddr, str::FromStr};

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};

use crate::{config::Admin, store::ServerStore};

pub async fn start_admin_server(admin_op: Option<Admin>, store: ServerStore) {
    if let Some(admin) = admin_op {
        let make_svc = make_service_fn(move |_conn| {
            let sc = store.clone();
            async move { Ok::<_, anyhow::Error>(service_fn(move |req| handle(req, sc.clone()))) }
        });

        let addr = SocketAddr::from_str(&admin.addr).expect("addr str parse err");
        let server = Server::bind(&addr).serve(make_svc);

        // Run this server for... forever!
        if let Err(e) = server.await {
            eprintln!("server error: {}", e);
        }
    }
}

async fn handle(req: Request<Body>, _store: ServerStore) -> anyhow::Result<Response<Body>> {
    let path = req.uri().path();
    match path {
        "/list" => Ok(resp("hi")),
        _ => Ok(error(400, "unsupport_path", "unsupport path")),
    }
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
