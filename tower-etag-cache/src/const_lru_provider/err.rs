// Error type must implement std::Error else axum will throw
// `the trait bound HandleError<...> is not satisfied`

use std::{error::Error, fmt::Display};

use tokio::sync::oneshot::error::RecvError;

#[derive(Debug)]
pub enum ConstLruProviderError {
    OneshotRecv(RecvError),
    MpscSend,
    ReadResBody(Box<dyn Error + Send + Sync>),
}

impl Display for ConstLruProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OneshotRecv(e) => e.fmt(f),
            Self::MpscSend => write!(f, "MpscSend"),
            Self::ReadResBody(e) => e.fmt(f),
        }
    }
}

impl Error for ConstLruProviderError {}
