use actix_web::{web, HttpResponse, HttpResponseBuilder, ResponseError};
use thiserror::Error;

use crate::error::internal as internal_error;

#[derive(Error, Debug)]
pub enum UserError {
	#[error("file not found")]
	NotFound,
}

impl ResponseError for UserError {
	fn status_code(&self) -> reqwest::StatusCode {
		match self {
			Self::NotFound => reqwest::StatusCode::NOT_FOUND,
		}
	}
}

type GetRawError = crate::error::Error<UserError>;

pub fn config(cfg: &mut web::ServiceConfig) {
	cfg.service(get_raw);
}

#[actix_web::get("/raw/{share_link}/{filename}")]
pub async fn get_raw(
	path: web::Path<(String, String)>,
	reqwest: web::Data<reqwest::Client>,
	config: web::Data<crate::Config>,
) -> Result<HttpResponse, GetRawError> {
	let (share_link, _filename) = &*path;
	let server = &config.seafile_server;

	log::debug!("Raw file requested for share link: '{share_link}'");

	// -- get file link from share link (unstable, don't cache)
	let file_link = {
		let req = reqwest
			.get(&format!("{server}/f/{share_link}/"))
			.query(&[("dl", "1")])
			.send()
			.await
			.map_err(|e| internal_error!(e))?;

		match req.status() {
			reqwest::StatusCode::FOUND => match req.headers().get(reqwest::header::LOCATION) {
				Some(x) => x.to_str().map_err(|e| internal_error!(e))?.to_owned(),
				None => return Err(internal_error!(req).into()),
			},
			reqwest::StatusCode::NOT_FOUND => return Err(UserError::NotFound.into()),
			_ => return Err(internal_error!(req).into()),
		}
	};
	log::trace!("Backing address for share link '{share_link}' is '{file_link}'");

	// -- get file content stream from backing file
	let backing_request = reqwest
		.get(file_link)
		.send()
		.await
		.map_err(|e| internal_error!(e))?;

	let backing_request_status = backing_request.status();
	let backing_stream = backing_request.bytes_stream();

	// TODO: resumable download headers
	// TODO: mime type based on `_filename` extension
	Ok(HttpResponseBuilder::new(backing_request_status)
		.insert_header((actix_web::http::header::CONTENT_TYPE, "application/octet-stream"))
		.streaming(backing_stream))
}
