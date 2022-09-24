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
	host: String,
	port: u16,
	seafile_server: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	env_logger::init();

	let config = {
		let args = std::env::args();
		let config_file = match args.len() {
			1 => "config.toml".to_owned(),
			2 => args.into_iter().skip(1).next().unwrap().to_owned(),
			_ => {
				println!("Usage: seashare [config_file]");
				std::process::exit(1);
			},
		};

		toml::from_str::<Config>(
			&std::fs::read_to_string(config_file).expect("could not load config file"),
		)?
	};

	let reqwest = reqwest::Client::builder()
		.redirect(reqwest::redirect::Policy::custom(|attempt| attempt.stop()))
		.build()
		.unwrap();

	let Config { host, port, .. } = config.clone();
	HttpServer::new(move || {
		App::new()
			.wrap(Logger::default())
			.wrap(middleware::NormalizePath::trim())
			.app_data(web::Data::<Config>::new(config.clone()))
			.app_data(web::Data::<reqwest::Client>::new(reqwest.clone()))
			.service(web::scope("").configure(endpoint::config))
	})
	.bind((host, port))?
	.run()
	.await
}
