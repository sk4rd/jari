inputs:
{
  config,
  lib,
  pkgs,
  ...
}:

with lib;

let
  inherit (pkgs.stdenv.hostPlatform) system;
  cfg = config.services.jari;
  package = inputs.self.packages.${system}.jari;
in
{
  options.services.jari = {
    enable = mkEnableOption "Jari service";
    port = mkOption {
      type = types.port;
      default = 8080;
      description = "Port on which Jari service will listen.";
    };
    oidc = {
        clientId = mkOption {
          type = types.path;
          description = "Path to the OIDC client ID file.";
        };
        clientSecret = mkOption {
          type = types.path;
          description = "Path to the OIDC client secret file.";
        };
      };
    openFirewall = mkOption {
      type = types.bool;
      default = false;
      description = "Open ports in the firewall for jari";
    };
  };

  config = mkIf cfg.enable {
    users.users."jari" = {
      isSystemUser = true;
      group = "jari";
      home = "/var/lib/jari";
      createHome = true;
    };

    users.groups.jari = { };
  
    systemd.services.jari = {
      description = "Jari Service";
      after = [ "network.target" ];
      wantedBy = [ "multi-user.target" ];
      environment = {
        "OIDC_CLIENT_ID" = cfg.oidc.clientId;
        "OIDC_CLIENT_SECRET" = cfg.oidc.clientSecret;
      };
      serviceConfig = {
        ExecStart = "${package}/bin/jari --port ${toString cfg.port}";
        Restart = "always";
        User = "jari";
        Group = "jari";
        WorkingDirectory = "${config.users.users.jari.home}";
      };
    };

    networking.firewall.allowedTCPPorts = mkIf cfg.openFirewall [ cfg.port ];
  };
}
