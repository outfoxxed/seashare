mod seafile_token;
mod upload;

use actix_web::web;

use self::seafile_token::*;

pub fn config(cfg: &mut web::ServiceConfig) {
	cfg.service(upload::upload);
}
