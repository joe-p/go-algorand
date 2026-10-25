# Adversarial WASM validation bounds

This is the first measurement pass for the question:

> Can adversarial workloads stay within acceptable validation time and memory?

The acceptance ceiling is the **strict maximum TEAL block**, on this machine,
not a reference-hardware target. A WASM workload passes if its measured
validation time and peak RSS stay below the TEAL maximum for the same block
limits. Absolute wall-clock suitability is reported separately, because the
strict TEAL maximum itself is far above a normal round time.

## Machine

- linux/arm64, Apple 8-core VM, 23 GiB RAM
- Go 1.25.3, Rust 1.97.1
- `wasm/target/release/libwasm_lsig.a`
- `GOMAXPROCS`/execpool parallelism: 8
- Warm allocator and OS caches unless noted. Peak RSS is `VmHWM`.

## Strict TEAL ceiling

The strict TEAL program is not the fib loop. A sweep of budget-filling TEAL
patterns found that fixed-cost O(n) byte operations dominate:

| TEAL pattern | charged cost | time per txn | ns/cost |
| --- | ---: | ---: | ---: |
| `b~` on 4 KiB | 19,994 | ~2.6–4.1 ms | ~130–208 |
| `b&` on two 4 KiB values | 19,989 | ~2.2–3.3 ms | ~109–165 |
| `bzero` 4 KiB | 19,986 | ~0.8–3.4 ms | ~38–171 |
| `concat` two 2 KiB values | 19,992 | ~1.8–3.3 ms | ~89–167 |
| `substring3` 4 KiB | 19,985 | ~0.21–0.25 ms | ~10–12 |
| `extract3` 4 KiB | 19,985 | ~0.21 ms | ~11 |
| `keccak256` 4 KiB | 19,996 | ~1.15 ms | ~57–58 |
| `sha512_256` 4 KiB | 19,996 | ~1.0–1.1 ms | ~50–55 |
| `sha256` 4 KiB | 19,971 | ~0.7–0.8 ms | ~35–41 |

`b~` and `b&` are both in the same fixed-cost O(n) family and the highest
measured; `b~` is used as the strict ceiling. Its fixed cost is 4 while it
copies and complements 4 KiB. This is a pre-existing AVM pricing property, not
a WASM property.

### Full strict TEAL block

Built with 36,924 `b~` LogicSig transactions, 5,242,819 bytes of payset
(the 5 MiB limit), and validated on the same 8-worker path as normal blocks:

| metric | value |
| --- | ---: |
| transactions | 36,924 |
| payset bytes | 5,242,819 / 5,242,880 |
| charged cost per txn | 19,994 |
| total charged cost | ~738 million |
| validation wall time | **45.4–47.5 s** (two full runs) |
| wall ns per charged cost | ~61–64 ns |
| peak RSS (`VmHWM`) | **1.88–1.97 GB** |

A 1,000-transaction block measured 1.13–1.29 s (median ~1.17 s), which
extrapolates to ~43–49 s and is consistent with the full runs. The cheap-program
block fits 41,613 transactions; the worst program's larger encoding reduces the
exact maximum to 36,924.

At block level, three fixed-cost O(n) patterns are within run noise of each
other and define the same ceiling:

| TEAL pattern | 1,000-txn wall | extrapolated max |
| --- | ---: | ---: |
| `bzero` 4 KiB | 1.18–1.21 s | 44.5–45.7 s |
| `concat` 4 KiB | 1.18–1.24 s | 41.7–44.1 s |
| `b~` 4 KiB | 1.18 s | 43.7 s |

The 1,000-transaction extrapolations slightly understate the full runs
(45.4–47.5 s), because parallel/GC behavior is not perfectly linear.

The full-block run quoted above uses `b~`; `bzero` and `concat` are within
measurement variance of the same value. The ceiling is therefore the
fixed-cost O(n) family, not a single opcode.

## WASM results

### Block level

Each row is a max-byte block, validated through `eval.Eval` with the real
verification worker pool. Times are medians of three runs; peak RSS is from a
fresh process for that case.

| WASM workload | txns / groups | validation wall | ns/txn | peak RSS |
| --- | ---: | ---: | ---: | ---: |
| `arg` 4 KiB, pooled groups of 16 | 10,816 / 676 | **1.11–1.15 s** | ~103,000–111,000 | **0.50 GB** |
| `memory.copy` 2 MiB | 17,361 | **0.98–1.04 s** | ~56,000–65,000 | **0.85–0.87 GB** |
| `memory.fill` 4 MiB | 17,594 | **0.59–0.63 s** | ~34,000–40,000 | **0.97–1.02 GB** |

The `arg` case uses one 4 KiB-argument LogicSig plus 15 plain payments per
group. The group-pooled budget is 320,000 units, so one program can read the
4 KiB argument up to the pooled limit. It is the worst host-payload block found.

### Per-invocation WASM costs

Measured with a 20,000-unit LogicSig budget unless noted:

| workload | max work | cost | time | ns/cost |
| --- | ---: | ---: | ---: | ---: |
| `arg` 4 KiB | 4,453 reads | 20,000 | 0.58–0.86 ms | ~12–36 |
| `memory.copy` 2 MiB | 5 copies | 19,456 | 196–235 µs | ~10–12 |
| `memory.fill` 4 MiB | 2 fills | 16,170 | 53–67 µs | ~2–4 |
| `memory.grow` 1→64 pages | to 4 MiB | 7,257 | 93 µs warm / 829 µs cold | ~13–114 |
| runaway LSig (out of fuel) | full budget | 20,000 | 0.33–0.53 ms | ~16–26 |
| failed clear-state app | 700 budget | 700 | 21–67 µs | ~30–96 |

The `memory.grow` result is the highest WASM ns/cost outside the `arg` case,
but it remains below the strict TEAL `b~` ceiling.

### Static validation

A 15,023-byte module with 1,350 functions is structurally valid but has a
setup charge above 20,000 units, so execution fails while static validation
still compiles it.

| metric | value |
| --- | ---: |
| `CheckSignature` per module | 206–258 µs |
| modules at the 5 MiB block limit | 348 |
| worst-block static-validation upper bound | **~72–90 ms** |

This work is not charged to the evaluation budget, but the block byte limit
keeps it small relative to the ~48 s strict TEAL ceiling. A block that actually
executes these modules cannot do so within the 20,000 budget.

## Useful contracts

| contract | WASM cost | TEAL cost | normal budget | pooled calls needed |
| --- | ---: | ---: | ---: | ---: |
| ed25519 LSig | 3,821 | 1,903 | 20,000 | 1 |
| `app_global_inc` | 943 | 10 | 700 | 2 |
| `app_fib N=0` | 921 | — | 700 | 2 |

The ed25519 LogicSig fits comfortably. A trivial WASM application does not fit
one default 700-unit app call: setup alone is 921–943 cost. That is a real
practicality constraint, even though 2 pooled calls is bounded. This is the
main “safe pricing makes useful contracts impractical” finding so far.

## Interpretation

On this machine, for the measured workloads:

- **Time:** strict max WASM validation is 1.11–1.15 s versus **45.4–47.5 s**
  for strict max TEAL. WASM is well inside the existing AVM worst-case envelope.
- **Memory:** strict max WASM peak RSS is 0.97–1.02 GB versus **1.88–1.97 GB**
  for strict max TEAL. WASM is below the AVM envelope.
- **Static validation:** ~72–90 ms worst-case upper bound, not a bottleneck.
- **Failed and clear-state executions:** consume their budget but do not
  create a new worst case.
- **Useful applications:** the default 700-unit app budget is too small for
  even a trivial WASM app; useful apps need pooling or a WASM-specific budget.

The strict TEAL ceiling is not a reasonable *absolute* validation-time target:
a valid adversarial TEAL block takes ~48 s on this 8-core machine because of the
existing `b~` pricing. If the goal is “within one round,” both AVM and WASM fail;
if the goal is “WASM is no worse than the AVM,” WASM passes on these cases.

## Remaining gaps

- Only the LogicSig path is measured end-to-end. TEAL app-call blocks execute
  serially and are expected to be lower than the ~45–48 s LSig ceiling, but that
  is not yet measured, and inner transactions are not covered.
- One machine, arm64, warm caches. amd64, cold process/page cache, and
  cross-platform fuel agreement remain open.
- The strict TEAL maximum uses two full-block runs; scaled runs show variance of
  roughly ±10%.
- Static validation is measured per module and multiplied by the byte-limit
  count; a block-level cancellation measurement would refine it.
- Host-payload coverage is `arg` plus bulk memory. Other 4 KiB host payloads,
  app-mode payloads, and large box/state paths are not yet in the max block.
- `GOMAXPROCS` is 8 here; aggregate WASM memory on many-core nodes still needs
  measurement.

## How to reproduce

```bash
make wasmlib
make wasm-adversarial   # all harnesses except the ~48s full TEAL block

WASM_ADVERSARIAL=1 go test ./ledger -run '^TestTEALOpcodeWorst$' -v
WASM_ADVERSARIAL=1 go test ./ledger -run '^TestTEALPatternBlocks$' -v -timeout=0
WASM_ADVERSARIAL=1 go test ./ledger -run '^TestTEALWorstCase$' -v -timeout=0
TEAL_MAX_FULL=1 WASM_ADVERSARIAL=1 go test ./ledger -run '^TestTEALWorstCase$' -v -timeout=0
WASM_ADVERSARIAL=1 go test ./ledger -run '^TestWASMAdversarialBlocks$' -v -count=3 -timeout=0

WASM_ADVERSARIAL=1 go test ./data/transactions/logic -run '^TestWasmAdversarialCost$' -v
WASM_ADVERSARIAL=1 go test ./data/transactions/logic -run '^TestWasmStaticValidationCost$' -v
WASM_ADVERSARIAL=1 go test ./data/transactions/logic -run '^TestWasmFailureAndGrowthCost$' -v
WASM_ADVERSARIAL=1 go test ./data/transactions/logic -run '^TestWasmUsefulContractBudget$' -v
```
