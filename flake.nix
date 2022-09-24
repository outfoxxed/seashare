{
	inputs.nixpkgs.url = "nixpkgs";
	outputs = { self, nixpkgs }: {
		packages."x86_64-linux".default = 
		let
			nix = import nixpkgs {
				system = "x86_64-linux";
			};
		in with nix; rustPlatform.buildRustPackage rec {
			pname = "seashare";
			version = "0.1.0";

			src = ./.;
			cargoLock.lockFile = ./Cargo.lock;
			buildInputs = with pkgs; [ openssl.dev ];
			nativeBuildInputs = with pkgs; [ pkg-config ];
			PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
		};
	};
}

