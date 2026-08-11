{
  description = "Development environment for synapse_fbs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { nixpkgs, ... }:
    let
      tools = {
        package.version = "0.8.0";
        flatbuffers = {
          version = "25.12.19";
          commit = "7e163021e59cca4f8e1e35a7c828b5c6b7915953";
        };
        flatbuffers-build.version = "0.2.4+flatc-25.12.19";
        flatcc = {
          version = "0.6.1";
          commit = "d17e324e7e595272da486c5b9b20e848b78ba9ba";
        };
        mcap = {
          rust = "0.25.0";
          python = "1.4.0";
          javascript = "2.2.1";
          cpp = {
            version = "2.1.3";
            commit = "1420296ffcfdcde4b6894c0c1aba0ad083f93dde";
          };
        };
        typescript.version = "7.0.2";
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
      toolingFor =
        pkgs:
        let
          toolsToml = (pkgs.formats.toml { }).generate "synapse-fbs-tools.toml" tools;
          python = pkgs.python3.withPackages (
            ps: with ps; [
              build
              twine
            ]
          );
        in
        {
          packages = with pkgs; [
            cargo
            clippy
            cmake
            flatbuffers
            git
            github-cli
            gnumake
            nodejs_24
            pkg-config
            python
            rustc
            rustfmt
            stdenv.cc
          ];
          environment = {
            SYNAPSE_FBS_PACKAGE_VERSION = tools.package.version;
            SYNAPSE_FBS_FLATBUFFERS_VERSION = tools.flatbuffers.version;
            SYNAPSE_FBS_FLATCC_VERSION = tools.flatcc.version;
            SYNAPSE_FBS_FLATC = "${pkgs.flatbuffers}/bin/flatc";
            SYNAPSE_FBS_TOOLS_TOML = "${toolsToml}";
          };
        };
      mkApp =
        pkgs: tooling: name: commands:
        let
          program = pkgs.writeShellApplication {
            inherit name;
            runtimeInputs = tooling.packages;
            runtimeEnv = tooling.environment // {
              PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
            };
            text = ''
              repo_root="$(git rev-parse --show-toplevel)"
              cd "$repo_root"
              ${commands}
            '';
          };
        in
        {
          type = "app";
          program = "${program}/bin/${name}";
        };
    in
    {
      devShells = forAllSystems (
        pkgs:
        let
          tooling = toolingFor pkgs;
        in
        {
          default = pkgs.mkShell (
            {
              packages = tooling.packages;
              buildInputs = [ pkgs.openssl ];
              shellHook = ''
                echo "synapse_fbs $SYNAPSE_FBS_PACKAGE_VERSION toolchain loaded"
              '';
            }
            // tooling.environment
          );
        }
      );

      apps = forAllSystems (
        pkgs:
        let
          tooling = toolingFor pkgs;
          testCommands = ''
            cargo fmt --check --manifest-path xtask/Cargo.toml
            cargo clippy --locked --manifest-path xtask/Cargo.toml --all-targets -- -D warnings
            cargo run --locked --manifest-path xtask/Cargo.toml -- check
          '';
          packageCommands = ''
            cargo run --locked --manifest-path xtask/Cargo.toml -- ci "$@"
          '';
          testApp = mkApp pkgs tooling "synapse-fbs-test" testCommands;
          packagesApp = mkApp pkgs tooling "synapse-fbs-packages" packageCommands;
          ciApp = mkApp pkgs tooling "synapse-fbs-ci" (testCommands + packageCommands);
        in
        {
          test = testApp;
          packages = packagesApp;
          build = packagesApp;
          ci = ciApp;
          default = ciApp;
        }
      );

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
