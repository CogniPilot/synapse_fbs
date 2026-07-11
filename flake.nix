{
  description = "Development environment for synapse_fbs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { nixpkgs, ... }:
    let
      tools = {
        package.version = "0.7.0";
        flatbuffers = {
          version = "25.12.19";
          commit = "7e163021e59cca4f8e1e35a7c828b5c6b7915953";
        };
        flatbuffers-build.version = "0.2.4+flatc-25.12.19";
        flatcc = {
          version = "0.6.1";
          commit = "d17e324e7e595272da486c5b9b20e848b78ba9ba";
        };
        docs.mdbook = "0.5.3";
      };
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
            }
          )
        );
    in
    {
      devShells = forAllSystems (
        pkgs:
        let
          toolsToml = (pkgs.formats.toml { }).generate "synapse-fbs-tools.toml" tools;
          mdbook = pkgs.rustPlatform.buildRustPackage rec {
            pname = "mdbook";
            version = tools.docs.mdbook;
            src = pkgs.fetchCrate {
              inherit pname version;
              hash = "sha256-2j22rRehYPpyPk1REPhHnRZ05WP0KXcv5mlpMxC83yg=";
            };
            cargoHash = "sha256-m5Vp2RAcqyesFH/+k5UHyLCePL2neTNwyei5czt3GyM=";
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            doCheck = false;
          };
          python = pkgs.python3.withPackages (
            ps: with ps; [
              build
              twine
            ]
          );
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              clippy
              cmake
              git
              github-cli
              mdbook
              nodejs_24
              python
              rustc
              rustfmt
            ];
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];

            SYNAPSE_FBS_PACKAGE_VERSION = tools.package.version;
            SYNAPSE_FBS_FLATBUFFERS_VERSION = tools.flatbuffers.version;
            SYNAPSE_FBS_FLATCC_VERSION = tools.flatcc.version;
            SYNAPSE_FBS_MDBOOK_VERSION = tools.docs.mdbook;
            SYNAPSE_FBS_TOOLS_TOML = "${toolsToml}";

            shellHook = ''
              echo "synapse_fbs $SYNAPSE_FBS_PACKAGE_VERSION toolchain loaded"
            '';
          };
        }
      );

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
