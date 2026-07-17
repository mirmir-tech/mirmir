use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use clap::Args;

#[derive(Debug, Args)]
pub struct HttpArgs {
    #[arg(long, env = "MIRMIR_HOST", default_value_t = IpAddr::V4(Ipv4Addr::LOCALHOST))]
    pub host: IpAddr,
    #[arg(long, env = "MIRMIR_PORT", default_value_t = 8080)]
    pub port: u16,
}

impl HttpArgs {
    #[must_use]
    pub const fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}
