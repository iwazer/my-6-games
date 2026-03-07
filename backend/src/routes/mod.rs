pub mod errors;
pub mod games;
pub mod health;
pub mod pages;
pub mod shares;

use std::net::IpAddr;

use rocket::request::{self, FromRequest, Request};

/// Caddy (X-Forwarded-For) 経由でクライアント IP を取得するリクエストガード
pub struct ClientIp(pub IpAddr);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ClientIp {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, ()> {
        let ip = req
            .real_ip()
            .or_else(|| req.client_ip())
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        request::Outcome::Success(ClientIp(ip))
    }
}
