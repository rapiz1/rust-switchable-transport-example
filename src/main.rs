use std::{env, net::SocketAddr};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use tokio::{
    fs,
    io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use tokio_native_tls::{
    native_tls::{self, Certificate, Identity},
    TlsAcceptor, TlsConnector, TlsStream,
};

#[async_trait]
trait Transport {
    type Acceptor;
    type Stream: 'static + AsyncRead + AsyncWrite + Unpin + Send;

    //fn new(config: TransportConfig) -> Self;
    async fn bind(&self, addr: &String) -> Result<Self::Acceptor>;
    async fn accept(&self, a: &Self::Acceptor) -> Result<(Self::Stream, SocketAddr)>;
    async fn connect(&self, addr: &String) -> Result<Self::Stream>;
}

struct TcpTransport {}
impl TcpTransport {
    fn new() -> TcpTransport {
        TcpTransport {}
    }
}
#[async_trait]
impl Transport for TcpTransport {
    type Acceptor = TcpListener;
    type Stream = TcpStream;

    async fn bind(&self, addr: &String) -> Result<Self::Acceptor> {
        Ok(TcpListener::bind(addr).await?)
    }

    async fn accept(&self, a: &Self::Acceptor) -> Result<(Self::Stream, SocketAddr)> {
        let (s, a) = a.accept().await?;
        Ok((s, a))
    }

    async fn connect(&self, addr: &String) -> Result<Self::Stream> {
        let s = TcpStream::connect(addr).await?;
        Ok(s)
    }
}
pub struct TransportConfig {
    pub tls: Option<TlsConfig>,
}

pub struct TlsConfig {
    pub trusted_root: Option<String>,
    pub pkcs12: Option<String>,
    pub pkcs12_password: Option<String>,
    pub hostname: Option<String>,
}

struct TlsTransport {
    config: TlsConfig,
    connector: Option<TlsConnector>,
}

impl TlsTransport {
    async fn new(config: TlsConfig) -> Result<TlsTransport> {
        let connector = match config.trusted_root.as_ref() {
            Some(path) => {
                let s = fs::read_to_string(path).await?;
                let cert = Certificate::from_pem(&s.as_bytes())?;
                let connector = native_tls::TlsConnector::builder()
                    .add_root_certificate(cert)
                    .build()?;
                Some(TlsConnector::from(connector))
            }
            None => None,
        };

        Ok(TlsTransport { config, connector })
    }
}

#[async_trait]
impl Transport for TlsTransport {
    type Acceptor = (TcpListener, TlsAcceptor);
    type Stream = TlsStream<TcpStream>;

    async fn bind(&self, addr: &String) -> Result<Self::Acceptor> {
        let ident = Identity::from_pkcs12(
            &fs::read(self.config.pkcs12.as_ref().unwrap()).await?,
            self.config.pkcs12_password.as_ref().unwrap(),
        )
        .with_context(|| "Failed to create identitiy")?;
        let l = TcpListener::bind(addr)
            .await
            .with_context(|| "Failed to create tcp listener")?;
        let t = TlsAcceptor::from(native_tls::TlsAcceptor::new(ident).unwrap());
        Ok((l, t))
    }

    async fn accept(&self, a: &Self::Acceptor) -> Result<(Self::Stream, SocketAddr)> {
        let (conn, addr) = a.0.accept().await?;
        let conn = a.1.accept(conn).await?;

        Ok((conn, addr))
    }

    async fn connect(&self, addr: &String) -> Result<Self::Stream> {
        let conn = TcpStream::connect(&addr).await?;
        let conn = self
            .connector
            .as_ref()
            .unwrap()
            .connect(self.config.hostname.as_ref().unwrap_or(&addr), conn)
            .await?;
        Ok(conn)
    }
}

async fn send_hello<T: Transport>(transport: T) -> Result<()> {
    let mut conn = transport.connect(&String::from("127.0.0.1:2334")).await?;
    let req = "hello";
    conn.write_all(req.as_bytes()).await?;
    io::copy(&mut conn, &mut io::stdout()).await?;
    Ok(())
}

async fn echo<T: Transport>(mut conn: T::Stream) -> Result<()> {
    let mut buf = [0u8; 2048];
    loop {
        let c = conn.read(&mut buf).await?;
        if c == 0 {
            break;
        }
        conn.write_all(&buf).await?;
    }
    Ok(())
}

async fn serve_echo<T: Transport>(transport: T) -> Result<()> {
    let addr = String::from("0.0.0.0:2334");
    let l = transport
        .bind(&addr)
        .await
        .with_context(|| "Failed to bind")?;
    while let Ok((conn, addr)) = transport.accept(&l).await {
        println!("get incoming {:?}", addr);
        tokio::spawn(async move {
            let _ = echo::<T>(conn).await;
        });
    }
    Ok(())
}

async fn run<T: Transport>(transport: T, mode: String) -> Result<()> {
    match mode.as_ref() {
        "serve" => serve_echo::<T>(transport).await,
        "client" => send_hello::<T>(transport).await,
        _ => Err(anyhow!("unknown mode")),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let t = args[1].clone();
    let mode = args[2].clone();
    let config = TlsConfig {
        trusted_root: Some(String::from("ca-cert.pem")),
        pkcs12: Some(String::from("identity.pfx")),
        pkcs12_password: Some(String::from("1234")),
        hostname: Some(String::from("0.0.0.0")),
    };
    match t.as_ref() {
        "tcp" => run(TcpTransport::new(), mode).await,
        "tls" => run(TlsTransport::new(config).await?, mode).await,
        _ => Err(anyhow!("unknown transport")),
    }
}
