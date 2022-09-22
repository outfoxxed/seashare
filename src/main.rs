mod endpoint;
mod error;

use actix_web::{
	middleware::{self, Logger},
	web,
	App,
	HttpServer,
};

#[derive(serde::Deserialize, Clone)]
pub struct Config {
	seafile_server: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	env_logger::init();

	let config = toml::from_str::<Config>(
		&std::fs::read_to_string("config.toml").expect("could not load config file"),
	)?;

	let reqwest = reqwest::Client::builder()
		.redirect(reqwest::redirect::Policy::custom(|attempt| attempt.stop()))
		.build()
		.unwrap();

	HttpServer::new(move || {
		App::new()
			.wrap(Logger::default())
			.wrap(middleware::NormalizePath::trim())
			.app_data(web::Data::<Config>::new(config.clone()))
			.app_data(web::Data::<reqwest::Client>::new(reqwest.clone()))
			.service(web::scope("").configure(endpoint::config))
	})
	.bind(("127.0.0.1", 8080))?
	.run()
	.await
}
