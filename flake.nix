{
  description = "dmn - lightweight domain management daemon";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      cargoToml = builtins.fromTOML (builtins.readFile ./app/Cargo.toml);
      version = cargoToml.package.version;

      mkPackages =
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          webSrc = pkgs.lib.cleanSource ./app/src/web;

          webCss = pkgs.stdenv.mkDerivation {
            pname = "dmn-web-css";
            inherit version;
            src = webSrc;

            pnpmDeps = pkgs.fetchPnpmDeps {
              pname = "dmn-web-css";
              inherit version;
              src = webSrc;
              pnpm = pkgs.pnpm_10;
              fetcherVersion = 5;
              hash = "sha256-hoLM0S4MP20ybWOZxfT5lF27JetIyzzt4YnwK9FjkIg=";
            };

            nativeBuildInputs = [
              pkgs.nodejs
              pkgs.pnpm_10
              pkgs.pnpmConfigHook
            ];

            buildPhase = ''
              runHook preBuild
              pnpm build
              runHook postBuild
            '';

            installPhase = ''
              runHook preInstall
              install -Dm644 dist/index.css "$out/index.css"
              runHook postInstall
            '';
          };

          mkDmn =
            targetPkgs:
            let
              isStatic = targetPkgs.stdenv.hostPlatform.isStatic;
            in
            targetPkgs.rustPlatform.buildRustPackage {
              pname = cargoToml.package.name;
              inherit version;
              src = pkgs.lib.cleanSource ./app;
              cargoLock.lockFile = ./app/Cargo.lock;

              nativeBuildInputs = with pkgs; [
                cmake
                perl
                pkg-config
              ];

              # sqlx-macros compiles for the build host and links host sqlite.
              depsBuildBuild = [ pkgs.sqlite ];

              buildInputs =
                (with targetPkgs; [
                  sqlite
                  zlib
                  zstd
                ])
                ++ pkgs.lib.optional (!isStatic) targetPkgs.aws-lc;

              env.SQLX_OFFLINE = "true";

              preBuild = ''
                ${pkgs.lib.optionalString isStatic "unset AWS_LC_SYS_USE_SYSTEM"}
                mkdir -p src/web/dist
                cp ${webCss}/index.css src/web/dist/index.css
              '';

              # The Porkbun tests make live network calls to the Porkbun API.
              doCheck = false;

              meta = {
                description = "Lightweight domain management daemon";
                homepage = "https://github.com/v3xlabs/dmn";
                mainProgram = "dmn";
              };
            };

          dmn = mkDmn pkgs;
          static = mkDmn pkgs.pkgsStatic;
          dockerImage = pkgs.dockerTools.buildLayeredImage {
            name = "ghcr.io/v3xlabs/dmn";
            tag = "latest";

            contents = [
              dmn
              pkgs.dockerTools.caCertificates
              (pkgs.dockerTools.fakeNss.override {
                extraPasswdLines = [ "dmn:x:10001:10001:dmn:/data:/sbin/nologin" ];
                extraGroupLines = [ "dmn:x:10001:" ];
              })
            ];

            fakeRootCommands = ''
              mkdir -p ./tmp ./data
              chmod 1777 ./tmp
              chown 10001:10001 ./data
            '';

            config = {
              User = "10001:10001";
              WorkingDir = "/data";
              ExposedPorts = {
                "3000/tcp" = { };
              };
              Volumes = {
                "/data" = { };
              };
              Env = [
                "RUST_LOG=error,dmn=info"
                "DMN_CONFIG_PATH=/data/config.toml"
                "DMN_DATABASE_URL=sqlite:///data/db.sqlite"
                "TMPDIR=/tmp"
                "HOME=/data"
                "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
              ];
              Cmd = [
                "/bin/dmn"
                "server"
              ];
            };
          };
        in
        {
          inherit dmn webCss;
          default = dmn;
        }
        // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          inherit dockerImage static;
        };
    in
    {
      packages = forAllSystems mkPackages;

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages =
              with pkgs;
              [
                cargo
                clippy
                cmake
                nodejs
                perl
                pkg-config
                pnpm_10
                rust-analyzer
                rustc
                rustfmt
                sqlite
                sqlx-cli
              ]
              ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.gcc ];

            env.SQLX_OFFLINE = "true";
          };
        }
      );

      nixosModules = {
        dmn = import ./nix/module.nix self;
        default = self.nixosModules.dmn;
      };

      checks = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          nixosEval = nixpkgs.lib.nixosSystem {
            inherit system;
            modules = [
              self.nixosModules.default
              {
                system.stateVersion = "25.05";
                boot.loader.grub.enable = false;
                fileSystems."/" = {
                  device = "nodev";
                  fsType = "tmpfs";
                };
                services.dmn = {
                  enable = true;
                  environmentFile = "/run/secrets/dmn";
                  settings = {
                    calendar.enabled = true;
                  };
                };
              }
            ];
          };
          service = nixosEval.config.systemd.services.dmn;
        in
        pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          nixos-module-eval =
            pkgs.runCommand "dmn-nixos-module-eval"
              {
                execStart = service.serviceConfig.ExecStart;
                configPath = service.environment.DMN_CONFIG_PATH;
                databaseUrl = service.environment.DMN_DATABASE_URL;
                envFile = builtins.head service.serviceConfig.EnvironmentFile;
                stateDir = service.serviceConfig.StateDirectory;
                dynamicUser = if service.serviceConfig.DynamicUser then "true" else "false";
                protectSystem = service.serviceConfig.ProtectSystem;
              }
              ''
                test "$execStart" = "${self.packages.${system}.default}/bin/dmn server"
                test "$databaseUrl" = "sqlite:///var/lib/dmn/db.sqlite"
                test "$envFile" = "/run/secrets/dmn"
                test "$stateDir" = "dmn"
                test "$dynamicUser" = "true"
                test "$protectSystem" = "strict"
                grep -qx '\[calendar\]' "$configPath"
                grep -qx 'enabled = true' "$configPath"
                touch "$out"
              '';
        }
      );
    };
}
