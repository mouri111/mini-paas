use actix_web::{web, App, HttpResponse, HttpServer, Responder, HttpRequest};
use futures::prelude::*;
use redis::AsyncCommands;
use redis::aio::ConnectionLike;
use redis::Commands;
use std::sync::Mutex;
use url::Url;

// ref : https://github.com/actix/examples/blob/master/http-proxy/src/main.rs
async fn forward(
    redis: actix_web::web::Data<Mutex<redis::aio::Connection>>, 
    req: HttpRequest,
    body: actix_web::web::Bytes,
) -> Result<HttpResponse, actix_web::Error> {
    async fn get_target(redis: &mut redis::aio::Connection, host: &str) -> Result<String, Box<dyn std::error::Error>> {
        let t: String = redis.get(host).await?;
        Ok(t)
    }
    let host = req.headers().get("host").unwrap().to_str().unwrap();
    eprintln!("headhers = {:?}", req.headers());
    eprintln!("host = {:?}", host);
    if host == "localhost" {
        return Ok(HttpResponse::Ok().body("nyan")); 
    }
    let target = get_target(&mut redis.lock().unwrap(), req.headers().get("host").unwrap().to_str().unwrap()).await.unwrap();
    let mut new_url = Url::parse(&target).unwrap();
    new_url.set_path(req.uri().path());
    new_url.set_query(req.uri().query());
    eprintln!("new_url = {}", new_url);

    let mut client = actix_web::client::Client::default();
    let forwarded_req = client
        .request_from(new_url.as_str(), req.head())
        .no_decompress();
    let forwarded_req = if let Some(addr) = req.head().peer_addr {
        forwarded_req.header("x-forwarded-for", format!("{}", addr.ip()))
    } else {
        forwarded_req
    };
    let mut res = forwarded_req.send_body(body).await.map_err(actix_web::Error::from)?;
    let mut client_resp = HttpResponse::build(res.status());
    for (header_name, header_value) in res.headers().iter().filter(|(h, _)| *h != "connection") {
        client_resp.header(header_name.clone(), header_value.clone());
    }
    Ok(client_resp.body(res.body().await?))
}

#[actix_rt::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let redis_client = redis::Client::open("redis://127.0.0.1")?;
    let redis_connection = redis_client.get_async_connection().await?;
    let redis_connection = actix_web::web::Data::new(Mutex::new(redis_connection));

    let server = HttpServer::new(move || {
        App::new()
            .app_data(redis_connection.clone())
            .default_service(web::route().to(forward))
    });
    server
        .bind("127.0.0.1:10080")?
        .run()
        .await?;
    Ok(())
}
