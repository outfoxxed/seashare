mod raw;
mod seafile_token;
mod upload;

use actix_web::web;

use self::seafile_token::*;

pub fn config(cfg: &mut web::ServiceConfig) {
	upload::config(cfg);
	raw::config(cfg);
}
