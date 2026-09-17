#!/usr/bin/env bash
#
# Regenerate FRAME weights for one or more bifrost pallets against the dev
# runtime wasm and write them back into each pallet's `src/weights.rs`.
#
# Runs `frame-omni-bencher` (same tool moonbeam uses). The dev runtime is the
# only one wired with `frame_benchmarking::define_benchmarks!`.
#
# ── frame-omni-bencher ────────────────────────────────────────────────────────
# It MUST be built from a polkadot-sdk that contains PR #10947 (revert of the
# #10802 `commit_db` change). The rev pinned in this workspace's Cargo.lock does
# NOT — with the broken bencher every pallet's `reads()/writes()` come out 0.
#   cargo install --git https://github.com/bifrost-platform/polkadot-sdk \
#     --branch bifrost-polkadot-stable2512 --locked frame-omni-bencher
# (works once #10947 is backported to that branch — see
#  memory/project_runtime_benchmarks.md).
#
# ── prerequisites ─────────────────────────────────────────────────────────────
#   cargo build --release -p bifrost-dev-runtime --features runtime-benchmarks
#
# ── usage ─────────────────────────────────────────────────────────────────────
#   scripts/benchmark-pallet.sh                       # default: pallet_tranche_permissions
#   scripts/benchmark-pallet.sh pallet_tranche_system pallet_tranche_tx_registry
#   STEPS=20 REPEAT=5 scripts/benchmark-pallet.sh     # faster, lower-precision
#   BENCHER=/path/to/frame-omni-bencher scripts/benchmark-pallet.sh
#
# Env: BENCHER, RUNTIME, STEPS, REPEAT, TEMPLATE, PALLET_DIR
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BENCHER="${BENCHER:-$(command -v frame-omni-bencher || true)}"
RUNTIME="${RUNTIME:-$ROOT/target/release/wbuild/bifrost-dev-runtime/bifrost_dev_runtime.compact.compressed.wasm}"
STEPS="${STEPS:-50}"
REPEAT="${REPEAT:-20}"
TEMPLATE="${TEMPLATE-$ROOT/scripts/frame-weight-template.hbs}"

[ -n "$BENCHER" ] && [ -x "$BENCHER" ] || { echo "frame-omni-bencher not found (set BENCHER=...)" >&2; exit 1; }
[ -f "$RUNTIME" ] || { echo "runtime wasm not found: $RUNTIME" >&2; exit 1; }

pallets=("$@")
[ ${#pallets[@]} -eq 0 ] && pallets=(pallet_tranche_permissions)

for p in "${pallets[@]}"; do
	# pallet_tranche_permissions -> pallets/tranche-permissions/src/weights.rs
	dir="${PALLET_DIR:-pallets/$(printf '%s' "${p#pallet_}" | tr '_' '-')}"
	out="$ROOT/$dir/src/weights.rs"
	[ -d "$ROOT/$dir" ] || { echo "no pallet dir for $p ($dir)" >&2; exit 1; }
	echo ">>> $p -> $dir/src/weights.rs"

	args=(
		v1 benchmark pallet
		--runtime "$RUNTIME"
		--genesis-builder runtime --genesis-builder-preset development
		--allow-missing-host-functions            # runtime imports unused ext_bifrost_ext_* HFs
		--pallet "$p" --extrinsic '*'
		--steps "$STEPS" --repeat "$REPEAT"
		--wasm-execution compiled
		--output "$out"
	)
	[ -n "$TEMPLATE" ] && args+=(--template "$TEMPLATE")
	"$BENCHER" "${args[@]}"
done

echo ">>> done. sanity-check that reads()/writes() are non-zero, then: cargo fmt"
