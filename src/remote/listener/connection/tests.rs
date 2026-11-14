use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

use super::*;

#[tokio::test]
async fn dropping_the_closer_fails_a_pending_read_and_later_writes() -> io::Result<()> {
    let closer = Closer::new();
    let (local, mut peer) = duplex(64);
    let mut connection = closer.handle().wrap(local);
    peer.write_all(b"ping").await?;
    let mut buffer = [0_u8; 4];
    connection.read_exact(&mut buffer).await?;
    assert_eq!(&buffer, b"ping");

    let reader = tokio::spawn(async move {
        let mut byte = [0_u8; 1];
        let read = connection.read(&mut byte).await;
        (connection, read)
    });
    tokio::task::yield_now().await;
    drop(closer);
    let (mut connection, read) = tokio::time::timeout(Duration::from_secs(1), reader)
        .await
        .map_err(io::Error::other)?
        .map_err(io::Error::other)?;
    assert_eq!(read.map_err(|error| error.kind()), Err(io::ErrorKind::ConnectionAborted));
    let written = connection.write_all(b"late").await;
    assert_eq!(written.map_err(|error| error.kind()), Err(io::ErrorKind::ConnectionAborted));
    Ok(())
}
