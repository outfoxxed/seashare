use actix_web::{
	http,
	web::{self, Bytes},
	ResponseError,
};
use futures_util::TryStreamExt;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::SeafileToken;
use crate::error::internal as internal_error;

type UploadError = crate::error::Error<UserError>;

#[derive(Error, Debug)]
pub enum UserError {
	#[error("no file submitted")]
	NoFileSubmitted,
	#[error("filename not specified")]
	FilenameNotSpecified,
	#[error("invalid seafile-token header")]
	InvalidToken,
	// note: the seafile docs don't actually say why this would
	// happen instead of `401: Unauthorized`
	#[error("permission denied")]
	PermissionDenied,
	#[error("no storage remaining")]
	QuotaFull,

	#[error("connection dropped")]
	ConnectionDropped,
	#[error("error reading multipart form")]
	MultipartError,
}

impl ResponseError for UserError {
	fn status_code(&self) -> http::StatusCode {
		use http::StatusCode;

		match self {
			Self::NoFileSubmitted
			| Self::MultipartError
			| Self::ConnectionDropped
			| Self::FilenameNotSpecified => StatusCode::BAD_REQUEST,
			Self::InvalidToken | Self::PermissionDenied => StatusCode::UNAUTHORIZED,
			Self::QuotaFull => StatusCode::INSUFFICIENT_STORAGE,
		}
	}
}

#[derive(Error, Debug)]
pub enum InternalError {
	#[error("unable to upload file to link: '{0}'")]
	BrokenUploadLink(String),
	#[error("malformed share link response: '{0}'")]
	MalformedShareLink(String),
}

#[actix_web::post(
	"/upload/{library:[0-9a-f]{8}-[0-9a-f]{4}-[0-5][0-9a-f]{3}-[089ab][0-9a-f]{3}-[0-9a-f]{12}}"
)]
pub async fn upload(
	library: web::Path<String>,
	seafile_token: web::Header<SeafileToken>,
	mut multipart: actix_multipart::Multipart,
	reqwest: web::Data<reqwest::Client>,
	config: web::Data<crate::Config>,
) -> Result<String, UploadError> {
	let library = library.into_inner();
	let seafile_token = seafile_token.into_inner().0;
	let server = &config.seafile_server;

	log::debug!("Client uploading to library {library}");

	// -- acquire multipart field
	let (mut multipart_field, submitted_filename) = {
		let mut multipart_field = None::<(actix_multipart::Field, String)>;
		while let Some(field) = multipart
			.try_next()
			.await
			.map_err(|_| UserError::MultipartError)?
		{
			let content_disposition = field.content_disposition();
			if content_disposition.get_name() != Some("file") {
				continue
			}
			let filename = match content_disposition.get_filename() {
				Some(x) => x.to_owned(),
				None => return Err(UserError::FilenameNotSpecified.into()),
			};

			multipart_field = Some((field, filename));
			break
		}

		match multipart_field {
			Some(x) => x,
			None => return Err(UserError::NoFileSubmitted.into()),
		}
	};

	// -- send file to seafile server
	let auth_header = format!("Token {}", seafile_token);
	let uploaded_filename = uuid::Uuid::new_v4().to_string();

	let (channel, send_task) = {
		// -- acquire seafile upload link
		let quoted_upload_link = {
			log::trace!("Querying upload link for library {library}");
			let link_request = reqwest
				.get(format!("{server}/api2/repos/{library}/upload-link/"))
				.header(reqwest::header::AUTHORIZATION, &auth_header)
				.send()
				.await
				.map_err(|e| internal_error!(e))?;

			match link_request.status() {
				reqwest::StatusCode::OK => {}
				reqwest::StatusCode::UNAUTHORIZED => Err(UserError::InvalidToken)?,
				reqwest::StatusCode::FORBIDDEN => Err(UserError::PermissionDenied)?,
				reqwest::StatusCode::INTERNAL_SERVER_ERROR => Err(UserError::QuotaFull)?,
				_ => Err(internal_error!(&link_request))?,
			}

			link_request.text().await.map_err(|e| internal_error!(e))?
		};
		let upload_link = &quoted_upload_link[1..quoted_upload_link.len() - 1];
		log::trace!("Upload link for library {library} is '{upload_link}'");

		// -- initiate upload request
		let (sender, receiver) = mpsc::channel::<Result<Bytes, UserError>>(1);

		let upload_form = reqwest::multipart::Form::new()
			.part("parent_dir", reqwest::multipart::Part::text("/"))
			.part(
				"file",
				reqwest::multipart::Part::stream(reqwest::Body::wrap_stream(ReceiverStream::new(
					receiver,
				)))
				.file_name(uploaded_filename.clone()),
			);

		let upload_request = reqwest
			.post(upload_link)
			.header(reqwest::header::AUTHORIZATION, &auth_header)
			.multipart(upload_form);

		if sender.is_closed() {
			// seafile gave a bad URL, so reqwest dropped the receiver
			return Err(internal_error!(InternalError::BrokenUploadLink(upload_link.to_string())))
		}

		let send_task = tokio::task::spawn(upload_request.send());

		(sender, send_task)
	};

	// -- flip multipart data from client to seafile server
	let file_id = loop {
		match multipart_field.try_next().await {
			Ok(Some(chunk)) => match channel.send(Ok(chunk)).await {
				Ok(()) => {}
				Err(_) => {
					// if this channel has been dropped, the request has already
					// failed, as correctly finishing the request is impossible
					// before the `Ok(None)` branch
					match send_task.await {
						Err(e) => return Err(internal_error!(e).into()),
						Ok(Err(e)) => return Err(internal_error!(e).into()),
						Ok(_) => unreachable!(),
					}
				}
			},
			Err(_) => {
				// sending an error kills the request to the seafile server,
				// and seafile ignores the pending upload
				match channel.send(Err(UserError::ConnectionDropped)).await {
					Ok(()) => {
						// close the channel and make sure the request ended
						drop(channel);
						let _ = send_task.await;

						return Err(UserError::ConnectionDropped.into())
					}
					Err(e) => return Err(internal_error!(e).into()),
				}
			}
			Ok(None) => {
				// close channel to complete request
				drop(channel);

				let response = send_task
					.await
					.map_err(|e| internal_error!(e))?
					.map_err(|e| internal_error!(e))?;

				let status = response.status();
				if status != reqwest::StatusCode::OK {
					return Err(internal_error!(response).into())
				}

				break response.text().await.map_err(|e| internal_error!(e))?
			}
		}
	};
	log::trace!("Uploaded file '{submitted_filename}', seafile id: '{file_id}'");

	// -- get share link from seafile server
	let share_link = {
		let share_link_json = reqwest
			.post(&format!("{server}/api/v2.1/share-links/"))
			.header(reqwest::header::AUTHORIZATION, &auth_header)
			.header(reqwest::header::ACCEPT, "application/json")
			.form(&[
				("repo_id", &library),
				("path", &&format!("/{uploaded_filename}")),
			])
			.send()
			.await
			.map_err(|e| internal_error!(e))?
			.text()
			.await
			.map_err(|e| internal_error!(e))?;

		let json_form =
			serde_json::from_str::<serde_json::Value>(&share_link_json).map_err(|_| {
				internal_error!(InternalError::MalformedShareLink(share_link_json.clone()))
			})?;

		json_form["link"]
			.as_str()
			.ok_or_else(|| internal_error!(InternalError::MalformedShareLink(share_link_json)))?
			.to_owned()
	};
	log::trace!("Created share link for upload '{submitted_filename}': '{share_link}'");

	Ok(share_link)
}
