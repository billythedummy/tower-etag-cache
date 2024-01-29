use std::{
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use http_body::{Body, Frame};
use pin_project::pin_project;

/// Newtype for Bytes to impl Body for
#[derive(Debug)]
#[pin_project]
pub struct ConstLruProviderTResBody(Bytes);

impl From<Bytes> for ConstLruProviderTResBody {
    fn from(value: Bytes) -> Self {
        Self(value)
    }
}

impl Body for ConstLruProviderTResBody {
    type Data = Bytes;

    type Error = Infallible;

    fn poll_frame(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let b = std::mem::take(self.project().0);
        if b.is_empty() {
            Poll::Ready(None)
        } else {
            Poll::Ready(Some(Ok(Frame::data(b))))
        }
    }
}
