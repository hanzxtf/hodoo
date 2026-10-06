# hodoo: a Rust client and CLI for Odoo 19's JSON-2 API.
#
# Thin wrappers around Cargo -- nothing here edits a server. Run `just` to list
# the recipes. The ones that touch a server read ODOO_URL/ODOO_API_KEY from the
# environment or a .env at the repo root.

set shell := ["bash", "-uc"]

# Recipe arguments reach the shell as "$@", so `just run -- --name "two words"`
# arrives as one argument instead of three.

set positional-arguments

# Cargo is not always on PATH: it usually lives in ~/.cargo/bin here.

cargo_env := 'PATH="$PATH:$HOME/.cargo/bin"'
manifest := justfile_directory() / "Cargo.toml"
debug_bin := justfile_directory() / "target/debug/hodoo"
release_bin := justfile_directory() / "target/release/hodoo"

# List the recipes
default:
    @just --list

# Build the client (debug); quiet, because it is a step in other recipes
build:
    @{{ cargo_env }} cargo build --quiet --manifest-path {{ manifest }}

# Run the client: just run -- task ls --project acme ("--" is optional)
run *ARGS: build
    @if [ "${1:-}" = "--" ]; then shift; fi; {{ debug_bin }} "$@"

# Formatting, lints and the offline tests: run this before pushing
check:
    {{ cargo_env }} cargo fmt --manifest-path {{ manifest }} --all --check
    {{ cargo_env }} cargo clippy --manifest-path {{ manifest }} --all-targets -- -D warnings
    {{ cargo_env }} cargo test --manifest-path {{ manifest }}

# The offline test suite alone: units, stub-server, help text, end-to-end UX
test:
    {{ cargo_env }} cargo test --manifest-path {{ manifest }}

# The two suites that need a real server: they read .env or the environment
live-test:
    HODOO_LIVE=1 {{ cargo_env }} cargo test --manifest-path {{ manifest }} -- --ignored --nocapture

# Release build, against the committed lock file
prod-build:
    {{ cargo_env }} cargo build --release --locked --manifest-path {{ manifest }}

# Install the release binary onto PATH: needs root for /usr/local/bin
install prefix="/usr/local/bin": prod-build
    sudo install -D -m 0755 {{ release_bin }} {{ prefix }}/hodoo
    @echo "installed {{ prefix }}/hodoo"
    @{{ prefix }}/hodoo --version

# Is the client ready? Toolchain, binary, credentials, server, and who the key is
doctor:
    #!/usr/bin/env bash
    set -uo pipefail
    printf '%-12s' 'cargo'
    cargo --version 2>/dev/null || "$HOME/.cargo/bin/cargo" --version 2>/dev/null || echo 'not found (install Rust 1.85+)'
    printf '%-12s' 'binary'
    if [ -x '{{ debug_bin }}' ]; then
      echo '{{ debug_bin }}'
    else
      echo 'not built yet: just build'
    fi
    printf '%-12s' 'credentials'
    if [ -f .env ]; then
      # Names only: a key must never be printed.
      grep -oE '^[A-Z_]+=' .env | tr -d '=' | paste -sd' ' -
    else
      echo 'no .env: ODOO_URL and ODOO_API_KEY must come from the environment'
    fi
    printf '%-12s' 'output'
    echo "HODOO_OUTPUT=${HODOO_OUTPUT:-table (default)}"
    if [ -x '{{ debug_bin }}' ]; then
      printf '%-12s' 'server'
      if '{{ debug_bin }}' -q odoo-version 2>/dev/null; then
        printf '%-12s' 'identity'
        '{{ debug_bin }}' -q whoami -o json 2>/dev/null |
          python3 -c 'import json,sys; d=json.load(sys.stdin); print(f"{d["name"]} (uid {d["uid"]})")' ||
          echo 'could not identify the key: check --url/ODOO_URL and the key'
      else
        echo 'could not reach the server: check --url/ODOO_URL and the certificate (--insecure)'
      fi
    fi
    printf '%-12s' 'scenario'
    if [ "$({{ debug_bin }} project ls '(scenario)' --limit 0 --no-headers 2>/dev/null | wc -l)" = '0' ]; then
      echo 'not loaded: just scenario up'
    else
      echo 'loaded: just scenario show'
    fi

# Remove build output and the scenario's scratch files under /tmp
clean:
    {{ cargo_env }} cargo clean --manifest-path {{ manifest }}
    rm -f /tmp/hodoo-scenario-auth.json /tmp/hodoo-scenario-error.json
    @echo 'cleaned target and the scenario scratch files'

# The startup-founder dataset in Odoo: just scenario up | down | show
scenario action="show":
    {{ justfile_directory() }}/scenarios/startup-founder.sh {{ action }}

# The iCare manager-DD dataset (15 workstreams, 79 tasks): just icare-dd up
icare-dd action="show":
    {{ justfile_directory() }}/scenarios/icare-dd.sh {{ action }}
