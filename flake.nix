{
  inputs = {
    nixpkgs.url = "https://channels.nixos.org/nixpkgs-unstable/nixexprs.tar.zst";
  };

  outputs = inputs: {
    devShells = builtins.mapAttrs (system: pkgs: {
      default = pkgs.mkShell {
        packages = [
          pkgs.discordchatexporter-cli
          pkgs.jq
        ];
      };
    }) inputs.nixpkgs.legacyPackages;
  };
}
