# TPT Vertex task runner — https://github.com/casey/just
#
#   just            # list recipes
#   just check      # type-check Rust + build the frontend
#   just fmt        # format Rust + frontend sources
#   just test       # run Rust + frontend test suites
#   just dev-frontend
#   just dev-desktop

# Show the available recipes.
default:
    @just --list

# Fast correctness pass: type-check the Rust workspace and build the web frontend.
check:
    @echo "==> cargo check --workspace"
    cargo check --workspace
    @echo "==> frontend build"
    npm --prefix frontend run build

# Format all sources. The frontend `format` script is optional: `--if-present`
# makes npm exit successfully when it is not defined.
fmt:
    @echo "==> cargo fmt --all"
    cargo fmt --all
    @echo "==> frontend format (skipped when the script is absent)"
    npm --prefix frontend run format --if-present

# Run the Rust workspace tests and the frontend (vitest) suite.
test:
    @echo "==> cargo test --workspace"
    cargo test --workspace
    @echo "==> frontend tests"
    npm --prefix frontend run test

# Start the Vite dev server for the web frontend.
dev-frontend:
    npm --prefix frontend run dev

# Start the Tauri desktop client in development mode.
dev-desktop:
    npm --prefix desktop run tauri dev
