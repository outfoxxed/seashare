use actix_web::{
	error::ParseError,
	http::header::{self, Header, HeaderName, HeaderValue, TryIntoHeaderValue},
	HttpMessage,
};
use reqwest::header::InvalidHeaderValue;

#[derive(Debug)]
pub struct SeafileToken(pub String);

impl Header for SeafileToken {
	fn name() -> HeaderName {
		HeaderName::from_lowercase(b"seafile-token").unwrap()
	}

	fn parse<M: HttpMessage>(msg: &M) -> Result<Self, ParseError> {
		let header = msg.headers().get(Self::name());
		header::from_one_raw_str(header).map(SeafileToken)
	}
}

impl TryIntoHeaderValue for SeafileToken {
	type Error = InvalidHeaderValue;

	fn try_into_value(self) -> Result<HeaderValue, Self::Error> {
		self.0.try_into_value()
	}
}
