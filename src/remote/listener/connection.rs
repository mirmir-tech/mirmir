//! Accepted connections that the listener can close by force. Tonic runs every
//! accepted connection and each of its streams on tasks of their own, so
//! stopping the serve future alone leaves them serving; their transport fails
//! instead once the listener's [`Closer`] is dropped.

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::StreamExt;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::watch,
};
use tokio_stream::wrappers::WatchStream;
use tonic::transport::server::Connected;

/// Closes every [`Closable`] connection it opened when dropped.
pub struct Closer {
    open: watch::Sender<()>,
}

impl Closer {
    pub fn new() -> Self {
        Self { open: watch::Sender::new(()) }
    }

    /// The accepting side's handle, which does not keep connections open.
    pub fn handle(&self) -> Opener {
        Opener { open: self.open.subscribe() }
    }
}

/// Wraps accepted connections so that they fail once the [`Closer`] is gone.
pub struct Opener {
    open: watch::Receiver<()>,
}

impl Opener {
    pub fn wrap<IO>(&self, io: IO) -> Closable<IO> {
        Closable {
            io,
            state: State::Open(WatchStream::from_changes(self.open.clone())),
        }
    }
}

enum State {
    Open(WatchStream<()>),
    Closed,
}

/// One accepted connection; every read and write fails after its [`Closer`]
/// is dropped, which ends the HTTP/2 connection and all of its streams.
pub struct Closable<IO> {
    io: IO,
    state: State,
}

impl<IO> Closable<IO> {
    /// Registers the task for the close signal and reports whether it fired.
    fn poll_closed(&mut self, cx: &mut Context<'_>) -> io::Result<()> {
        if let State::Open(signal) = &mut self.state {
            // The sender only ever closes; any item or the end means closed.
            if signal.poll_next_unpin(cx).is_pending() {
                return Ok(());
            }
            self.state = State::Closed;
        }
        Err(io::Error::new(io::ErrorKind::ConnectionAborted, "remote access stopped"))
    }
}

impl<IO: Connected> Connected for Closable<IO> {
    type ConnectInfo = IO::ConnectInfo;

    fn connect_info(&self) -> Self::ConnectInfo {
        self.io.connect_info()
    }
}

impl<IO: AsyncRead + Unpin> AsyncRead for Closable<IO> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.poll_closed(cx)?;
        Pin::new(&mut this.io).poll_read(cx, buf)
    }
}

impl<IO: AsyncWrite + Unpin> AsyncWrite for Closable<IO> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.poll_closed(cx)?;
        Pin::new(&mut this.io).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.poll_closed(cx)?;
        Pin::new(&mut this.io).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().io).poll_shutdown(cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.poll_closed(cx)?;
        Pin::new(&mut this.io).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.io.is_write_vectored()
    }
}

#[cfg(test)]
mod tests;
