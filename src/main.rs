use anyhow::Context;
use base64::Engine;
use memchr::memmem::Finder;
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time,
};
use tracing::{debug, error, info, warn};

const READ_BUF_SIZE: usize = 8192;
const CONNECT_TIMEOUT_SECS: u64 = 10;
const ACCEPT_RETRY_DELAY_MS: u64 = 50;

#[inline]
fn find_needle_in_read(
    finder: &Finder,
    read_buf: &[u8],
    n: usize,
    tail: &mut Vec<u8>,
    needle_len: usize,
) -> bool {
    if tail.is_empty() {
        if finder.find(&read_buf[..n]).is_some() {
            return true;
        }
        if needle_len > 1 {
            let keep = needle_len.saturating_sub(1).min(n);
            tail.extend_from_slice(&read_buf[n - keep..n]);
        }
        return false;
    }

    let tail_len = tail.len();
    tail.extend_from_slice(&read_buf[..n]);
    let total = tail.len();

    if finder.find(tail).is_some() {
        tail.truncate(tail_len);
        return true;
    }

    if needle_len > 1 {
        let keep = needle_len.saturating_sub(1).min(total);
        tail.drain(..total - keep);
    } else {
        tail.clear();
    }
    false
}

async fn handle_incoming(
    mut client: TcpStream,
    finder: &'static Finder<'static>,
    upstream_addr: SocketAddr,
    prepend: &'static [u8],
) -> anyhow::Result<()> {
    if let Err(e) = client.set_nodelay(true) {
        warn!("failed to set TCP_NODELAY on client socket: {}", e);
    }

    let needle_len = finder.needle().len();
    if needle_len > 0 {
        let mut tail: Vec<u8> = Vec::with_capacity(needle_len.saturating_sub(1));
        let mut read_buf = [0u8; READ_BUF_SIZE];

        loop {
            let n = client
                .read(&mut read_buf)
                .await
                .context("read from client failed while waiting for needle")?;

            if n == 0 {
                anyhow::bail!("Premature end of client stream while waiting for needle");
            }

            if find_needle_in_read(finder, &read_buf, n, &mut tail, needle_len) {
                debug!("found needle");
                break;
            }
        }
    }

    let mut upstream = time::timeout(
        Duration::from_secs(CONNECT_TIMEOUT_SECS),
        TcpStream::connect(upstream_addr),
    )
    .await
    .context("connect timeout")?
    .context("connect failed")?;

    info!("connected to upstream {}", upstream_addr);

    if !prepend.is_empty() {
        client
            .write_all(prepend)
            .await
            .context("writing prepend failed")?;
        debug!("wrote prepend bytes");
    }

    let (read_up, read_client) = tokio::io::copy_bidirectional(&mut upstream, &mut client)
        .await
        .context("proxy failed")?;

    debug!(
        "finished proxying (up->client={} bytes, client->up={})",
        read_up, read_client
    );

    Ok(())
}

xflags::xflags! {
    cmd Tcpprepend {
        required listen: SocketAddr
        required request_needle_base64: String
        required connect: SocketAddr
        required response_prepend_base64: String
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let opts = Tcpprepend::from_env_or_exit();

    let needle_vec = base64::engine::general_purpose::STANDARD
        .decode(opts.request_needle_base64)
        .context("failed to decode request needle")?;
    let prepend_vec = base64::engine::general_purpose::STANDARD
        .decode(opts.response_prepend_base64)
        .context("failed to decode response prepend")?;

    let needle: &'static [u8] = Box::leak(needle_vec.into_boxed_slice());
    let prepend: &'static [u8] = Box::leak(prepend_vec.into_boxed_slice());
    let finder: &'static Finder<'static> = Box::leak(Box::new(Finder::new(needle)));

    let listener = TcpListener::bind(opts.listen)
        .await
        .context("bind failed")?;
    info!("listening on {}", opts.listen);

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("shutdown requested; exiting accept loop");
        }
        _ = async {
            loop {
                let (socket, peer) = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        error!("accept error: {}", e);
                        time::sleep(Duration::from_millis(ACCEPT_RETRY_DELAY_MS)).await;
                        continue;
                    }
                };

                info!("incoming connection from {}", peer);

                tokio::spawn(handle_incoming(socket, finder, opts.connect, prepend));
            }
        } => {}
    }

    Ok(())
}
