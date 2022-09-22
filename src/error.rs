use std::marker::PhantomData;

use actix_web::{http::StatusCode, ResponseError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error<USER: ResponseError> {
	#[error(transparent)]
	User(#[from] USER),
	#[error("internal server error")]
	Internal(PhantomData<()>),
}

impl<USER: ResponseError> ResponseError for Error<USER> {
	fn status_code(&self) -> StatusCode {
		match self {
			Self::User(e) => e.status_code(),
			Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
		}
	}
}

#[macro_export]
macro_rules! internal {
	($error:expr) => {{
		::log::error!(
			"internal error occured: '{:#?}', {}:{}:{}",
			$error,
			file!(),
			line!(),
			column!()
		);
		$crate::error::Error::Internal(::std::marker::PhantomData)
	}};
}

pub use internal;
