{
  description = "Development environment for synapse_fbs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { nixpkgs, ... }:
    let
      tools = {
        package.version = "0.9.0";
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
            flatcc
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
            SYNAPSE_FBS_FLATCC = "${pkgs.flatcc}/bin/flatcc";
            SYNAPSE_FBS_FLATCC_SOURCE = "${pkgs.flatcc.src}";
            SYNAPSE_FBS_FLATC = "${pkgs.flatbuffers}/bin/flatc";
            SYNAPSE_FBS_TOOLS_TOML = "${toolsToml}";
          };
        };
      mkCommand =
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
        program;
      commandsFor =
        pkgs: tooling:
        let
          test = mkCommand pkgs tooling "synapse-fbs-test" ''
            cargo fmt --check --manifest-path xtask/Cargo.toml
            cargo clippy --locked --manifest-path xtask/Cargo.toml --all-targets -- -D warnings
            cargo run --locked --manifest-path xtask/Cargo.toml -- check
            cargo run --locked --manifest-path xtask/Cargo.toml -- wire-check --update
            git diff --exit-code compatibility/wire-schema.toml
          '';
          packages = mkCommand pkgs tooling "synapse-fbs-packages" ''
            cargo run --locked --manifest-path xtask/Cargo.toml -- ci "$@"
          '';
          ci = pkgs.writeShellApplication {
            name = "synapse-fbs-ci";
            runtimeInputs = [
              test
              packages
            ];
            text = ''
              synapse-fbs-test
              synapse-fbs-packages "$@"
            '';
          };
        in
        {
          inherit ci packages test;
        };
      # The generated bindings as immutable outputs rather than a directory
      # under target/. Consumers previously read the working tree while the
      # generator was rewriting it, so a build could lose headers midway
      # through. A store path cannot be rewritten, so a consumer holding one is
      # unaffected by any later generation.
      #
      # Generation itself stays in the existing xtask; this only runs it from
      # tracked source and keeps what it produces.
      bindingsFor =
        pkgs:
        let
          tooling = toolingFor pkgs;
        in
        pkgs.stdenv.mkDerivation (
          {
            pname = "synapse-fbs";
            version = tools.package.version;

            # Tracked source only. target/ is mutable build state and must not
            # enter a derivation.
            src = pkgs.lib.cleanSourceWith {
              src = ./.;
              filter =
                path: _type:
                let
                  relative = pkgs.lib.removePrefix (toString ./. + "/") (toString path);
                in
                !(pkgs.lib.hasPrefix "target" relative) && !(pkgs.lib.hasPrefix ".git" relative);
            };

            outputs = [
              "out"
              "c"
              "rust"
              "python"
              "js"
            ];

            nativeBuildInputs = tooling.packages ++ [ pkgs.rustPlatform.cargoSetupHook ];
            cargoDeps = pkgs.rustPlatform.importCargoLock { lockFile = ./xtask/Cargo.lock; };
            cargoRoot = "xtask";

            # CMake is a tool the generator invokes, not this derivation's build
            # system, so its setup hook must not claim the configure phase.
            dontUseCmakeConfigure = true;

            # The multiple-outputs hook relocates include/ to a development
            # output by default, which silently emptied the C bindings of every
            # generated header while leaving the rest of the tree intact. The
            # headers are the point of this output, so keep them in it.
            outputInclude = "c";

            buildPhase = ''
              runHook preBuild
              cargo run --locked --offline --manifest-path xtask/Cargo.toml -- \
                build --release-name ${tools.package.version}
              runHook postBuild
            '';

            installPhase = ''
              runHook preInstall
              cp -r target/xtask/artifacts-work/synapse_fbs-c "$c"
              cp -r target/xtask/packages/rust "$rust"
              cp -r target/xtask/packages/python "$python"
              cp -r target/xtask/packages/js "$js"
              # The default output is the coherent set: one link per ecosystem,
              # all from the same generation. Building the advertised package
              # otherwise yielded a directory containing only a version string.
              mkdir -p "$out"
              ln -s "$c" "$out/c"
              ln -s "$rust" "$out/rust"
              ln -s "$python" "$out/python"
              ln -s "$js" "$out/javascript"
              printf 'synapse_fbs %s\n' "${tools.package.version}" > "$out/version"
              runHook postInstall
            '';
          }
          // tooling.environment
        );

      asApp = name: program: {
        type = "app";
        program = "${program}/bin/${name}";
      };
    in
    {
      devShells = forAllSystems (
        pkgs:
        let
          tooling = toolingFor pkgs;
          commands = commandsFor pkgs tooling;
        in
        {
          default = pkgs.mkShell (
            {
              packages = tooling.packages ++ builtins.attrValues commands;
              buildInputs = [ pkgs.openssl ];
              shellHook = ''
                echo "synapse_fbs $SYNAPSE_FBS_PACKAGE_VERSION toolchain loaded"
                echo "Commands: synapse-fbs-test, synapse-fbs-packages, synapse-fbs-ci"
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
          commands = commandsFor pkgs tooling;
        in
        {
          test = asApp "synapse-fbs-test" commands.test;
          packages = asApp "synapse-fbs-packages" commands.packages;
          build = asApp "synapse-fbs-packages" commands.packages;
          ci = asApp "synapse-fbs-ci" commands.ci;
          default = asApp "synapse-fbs-ci" commands.ci;
        }
      );

      packages = forAllSystems (
        pkgs:
        let
          bindings = bindingsFor pkgs;
        in
        {
          synapse-fbs = bindings;
          # One generation, several consumable roots. Each alias names the
          # ecosystem a consumer asks for, so nobody has to know which output
          # of which derivation carries it.
          synapse-fbs-c = bindings.c;
          synapse-fbs-rust = bindings.rust;
          synapse-fbs-python = bindings.python;
          synapse-fbs-javascript = bindings.js;
          default = bindings;
        }
      );

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
