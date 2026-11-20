self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.dmn;
  tomlFormat = pkgs.formats.toml { };
  configFile = tomlFormat.generate "dmn-config.toml" cfg.settings;
in
{
  options.services.dmn = {
    enable = lib.mkEnableOption "dmn domain management daemon (listens on 0.0.0.0:3000)";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "inputs.dmn.packages.\${pkgs.stdenv.hostPlatform.system}.default";
      description = "The dmn package to run.";
    };

    databaseUrl = lib.mkOption {
      type = lib.types.str;
      default = "sqlite:///var/lib/dmn/db.sqlite";
      description = "SQLite database URL passed to dmn via DMN_DATABASE_URL.";
    };

    environmentFile = lib.mkOption {
      type = lib.types.nullOr (
        lib.types.pathWith {
          inStore = false;
          absolute = true;
        }
      );
      default = null;
      example = "/run/secrets/dmn";
      description = ''
        Path to a systemd EnvironmentFile containing runtime secrets such as
        DMN_API_SECRET, PORKBUN_API_KEY, PORKBUN_SECRET_KEY,
        CLOUDFLARE_API_KEY, CLOUDFLARE_GLOBAL_API_KEY, CLOUDFLARE_EMAIL,
        and NTFY_* variables.
        Loaded at runtime by systemd and never written to the Nix store.
      '';
    };

    settings = lib.mkOption {
      type = tomlFormat.type;
      default = { };
      example = lib.literalExpression ''
        {
          calendar.enabled = true;
          porkbun.pricing = {
            enabled = true;
            interval = "1d";
          };
        }
      '';
      description = ''
        Non-secret configuration rendered to config.toml and passed via DMN_CONFIG_PATH.
        Keep API keys and secrets in environmentFile instead because this file is stored in the Nix store.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.services.dmn = {
      description = "dmn domain management daemon";
      wantedBy = [ "multi-user.target" ];
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];

      environment = {
        DMN_CONFIG_PATH = "${configFile}";
        DMN_DATABASE_URL = cfg.databaseUrl;
        HOME = "/var/lib/dmn";
        XDG_CONFIG_HOME = "/var/lib/dmn";
        RUST_LOG = lib.mkDefault "error,dmn=info";
      };

      serviceConfig = {
        ExecStart = "${lib.getExe cfg.package} server";
        Restart = "on-failure";
        RestartSec = 5;

        DynamicUser = true;
        StateDirectory = "dmn";
        StateDirectoryMode = "0750";
        WorkingDirectory = "/var/lib/dmn";
        EnvironmentFile = lib.optional (cfg.environmentFile != null) cfg.environmentFile;

        NoNewPrivileges = true;
        PrivateTmp = true;
        PrivateDevices = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectKernelLogs = true;
        ProtectControlGroups = true;
        ProtectClock = true;
        ProtectHostname = true;
        RestrictAddressFamilies = [
          "AF_INET"
          "AF_INET6"
          "AF_UNIX"
        ];
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        SystemCallArchitectures = "native";
        CapabilityBoundingSet = "";
        AmbientCapabilities = "";
        UMask = "0077";
      };
    };
  };
}
