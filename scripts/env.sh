# araiguma environment — source this file in every working shell:
#
#     source scripts/env.sh
#
# It implements the durable-storage rules (WO-01R §3.1):
#   * Rust build output lives outside the persistent tree.
#   * Cargo's registry extraction lives outside the persistent tree; the
#     persistent crate-archive cache is reused through a symlink so that
#     downloads survive session boundaries without storing extracted trees.
#   * The npm cache lives outside the persistent tree.
# The file is safe to source repeatedly.

# Repository root (scripts/ parent).
ARIGUMA_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export ARIGUMA_ROOT

# Toolchain location. The default is a sibling of the repository
# (<parent>/toolchains), which is where the sandbox toolchain lives.
ARIGUMA_TOOLCHAINS="${ARIGUMA_TOOLCHAINS:-$(dirname -- "$ARIGUMA_ROOT")/toolchains}"
export ARIGUMA_TOOLCHAINS

# Rust: RUSTUP_HOME is persistent (it is the toolchain itself); CARGO_HOME is
# ephemeral so that registry/src extraction never lands in the persistent
# tree. The shims in <toolchains>/cargo/bin remain on PATH.
export RUSTUP_HOME="${RUSTUP_HOME:-$ARIGUMA_TOOLCHAINS/rustup}"
export CARGO_HOME="${ARIGUMA_CARGO_HOME:-/tmp/araiguma-cargo}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/araiguma-target}"
case ":$PATH:" in
  *":$ARIGUMA_TOOLCHAINS/cargo/bin:"*) ;;
  *) export PATH="$ARIGUMA_TOOLCHAINS/cargo/bin:$PATH" ;;
esac

# npm: the cache must stay out of the persistent tree.
export npm_config_cache="${npm_config_cache:-/tmp/araiguma-npm-cache}"

# Playwright browsers live in the ephemeral home cache by default.
export PLAYWRIGHT_BROWSERS_PATH="${PLAYWRIGHT_BROWSERS_PATH:-$HOME/.cache/ms-playwright}"

# Ephemeral layout + persistent crate-cache symlink (idempotent).
if [ -d "$RUSTUP_HOME" ]; then
  mkdir -p "$CARGO_HOME/registry" "$CARGO_TARGET_DIR" "$npm_config_cache" 2>/dev/null || true
  if [ -d "$ARIGUMA_TOOLCHAINS/cargo/registry/cache" ] && [ ! -e "$CARGO_HOME/registry/cache" ]; then
    ln -s "$ARIGUMA_TOOLCHAINS/cargo/registry/cache" "$CARGO_HOME/registry/cache"
  fi
fi
