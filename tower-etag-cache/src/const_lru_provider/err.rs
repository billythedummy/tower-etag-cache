// Error type must implement std::Error else axum will throw
// `the trait bound HandleError<...> is not satisfied`

use std::{
    error::Error,
    fmt::{Debug, Display},
};

use tokio::sync::oneshot::error::RecvError;

#[derive(Debug)]
pub enum ConstLruProviderError<ResBodyError> {
    OneshotRecv(RecvError),
    MpscSend,
    ReadResBody(ResBodyError),
}

impl<ResBodyError: Display> Display for ConstLruProviderError<ResBodyError> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OneshotRecv(e) => Display::fmt(&e, f),
            Self::MpscSend => write!(f, "MpscSend"),
            Self::ReadResBody(e) => Display::fmt(&e, f),
        }
    }
}

impl<ResBodyError: Debug + Display> Error for ConstLruProviderError<ResBodyError> {}
