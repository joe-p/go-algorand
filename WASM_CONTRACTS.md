# Compiler-generated WASM contracts: size and budget at normal limits

This answers the spike question:

> Can realistic, compiler-generated contracts fit actual size and budget limits?

**Short answer: size is not the binding constraint; budget is.** An optimized
Rust contract fits comfortably in the application page limits, but the
deterministic setup charge makes pooling mandatory even to *deploy* an app. A
trivial 560-byte Rust application needs three pooled app calls. A representative
signature-verifying contract needs **7 pooled app calls to deploy and 12 of the
maximum 16 to call**; spending all 16 slots would pay for only about one
additional signature check.

The experiment deploys and executes the fixtures through `algod` under an
unmodified `ConsensusFuture` parameter set. No benchmark caps are lifted:
`MaxAppProgramCost` stays 700, and extra budget comes only from adding app-call
transactions to the group. Simulation requests do not use
`extra-opcode-budget` or `allow-more-opcode-budget`.

## Limits under test

| limit | value |
| --- | ---: |
| `MaxAppProgramLen` | 2,048 bytes/page |
| free extra pages (`MaxExtraAppProgramPages`) | 3 (8,192 bytes total) |
| absolute extra pages in `vFuture` | 7 (16,384 bytes total) |
| per-application-call budget (`MaxAppProgramCost`) | 700 |
| maximum transactions per group (`MaxTxGroupSize`) | 16 |
| **maximum pooled app budget** | **11,200** |
| clear-state budget | 700, unpooled |
| `MaxAbsoluteLogicSigProgramSize` in `vFuture` | 16,000 |

Programs above 8,192 total bytes pay a per-byte fee, and each page beyond the
free four adds 0.1 ALGO of minimum balance. The WASM setup preflight
independently rejects modules above 16,384 bytes, so the absolute app ceiling is
16,384 bytes.

## Fixtures

Both are compiled from Rust with the guest workspace profile
(`opt-level="z"`, LTO, one codegen unit, `panic="abort"`, stripped):

- **`app_rust_counter`** — 8-byte big-endian delta argument, sender field read,
  global counter and per-sender total. No crypto, no allocation. This is the
  floor for a real stateful Rust application.
- **`app_rust_signed_claim`** — 64-byte ed25519 signature argument over the
  8-byte amount argument, verified through the patched `ed25519-dalek` API
  (which calls `env.ed25519verify_bare`), per-sender replay protection, and a
  cumulative cap in global state. This is representative of a signature-
  authorized contract.

Both return true early when `txn ApplicationID == 0`: the protocol executes the
approval program during creation too, so creating an app must not perform the
claim.

Because `goal` cannot sign arbitrary bytes, the issuer signature in the `goal`
reproduction is a fixed bearer voucher over the amount. It still exercises the
same argument, crypto, and state paths.

## Results

Measured on linux/arm64 (Apple 8-core VM, 23 GiB RAM), Go 1.25.3, Rust 1.97.1,
with warm caches. Costs are AVM budget units; fuel is wasmi's denomination at
10 fuel per unit.

| contract | bytes | pages / EPP | setup cost | deploy cost | call cost | deploy calls | call calls |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `app_rust_counter` | 560 | 1 / 0 | **1,652** | 1,928 | 2,075 | **3** | **3** |
| `app_rust_signed_claim` | 4,221 | 3 / 2 | **4,291** | 4,705 | 8,187 | **7** | **12** |

`deploy cost` and `call cost` are `app-budget-consumed` for the first
transaction of a successful group, reported by `simulate` through the new WASM
tracer hooks. `setup cost` is the deterministic charge from
`make wasm-setupcost`; execution is the remainder. Deploy and call include the
real approval execution, not just compilation. The Go evaluation at the same
normal 16-call pool reports the same costs as the algod simulation (2,075 for
the counter and 8,187 for the signed claim), so the pricing is consistent
between the harness and the node.

For comparison, the WAT `app_fib.wasm` fixture is 208 bytes with an 804 setup
cost, and the Rust `ed25519_dalek.wasm` LogicSig fixture is 4,761 bytes with a
4,546 setup cost. The WAT fixture understates compiler-generated applications
because it has no memory section, no runtime functions, and no data segments.

### Setup is the majority of a call

| contract | setup | deploy execution | call execution |
| --- | ---: | ---: | ---: |
| `app_rust_counter` | 1,652 | 276 | 423 |
| `app_rust_signed_claim` | 4,291 | 414 | 3,896 |

The signed-claim call execution is dominated by the 2,850-unit
`ed25519verify_bare` host charge. Even so, setup is 52% of the total call cost.
For the counter, setup is 80%.

The 17-page initial memory (Rust's default 1 MiB stack) alone costs 17 × 300 =
5,100 fuel = **510 units** on every evaluation. A guest build that sets a
smaller stack (`-C link-arg=-zstack-size=...`) would save roughly 480 of those
units. The remaining setup is byte pricing (6 fuel/byte), function/type/export
charges, and the fixed base.

## What "pooling" actually requires

Each application-call transaction in a group adds 700 units, regardless of which
app it calls. Payments do not add budget. The experiments use one real call plus
cheap no-op calls to a trivial AVM app (about 4 units each); their distinct notes
keep the group transaction IDs unique.

Consequences found through `algod`:

- **Deployment needs pooling too.** Creation executes the approval program, so
  the create transaction is itself subject to the 700-unit budget. The
  signed-claim app cannot be created in a single transaction; it needs a
  7-transaction group. The counter needs 3.
- **A single call always fails.** With one app call, the group has 700 units and
  the runtime fails during setup with `wasm program ran out of fuel during
  setup`. The e2e test asserts this for both fixtures.
- **The reported costs are minimal.** The e2e test finds the smallest N that
  succeeds and checks that N−1 fails, so the numbers are requirements, not
  slack. It also asserts the group's `app-budget-added` is exactly N × 700,
  proving no override.
- **12 of 16 slots for a realistic contract; a second verification needs all
  16.** One ed25519 verification costs 2,850 units. A second verification would
  put a call at about 11,037 units, just under the 11,200 maximum pool, so only
  a full 16-call group could pay for it and only about 160 units would remain.
  Even at the maximum pool just 3,013 units remain above the 8,187 call cost,
  and the minimal 12-call group has almost no slack.

Fees are not the constraint: minimum-fee groups cost 0.007 ALGO to deploy and
0.012 ALGO to call. The application minimum balance is 0.157 ALGO for the
one-page counter and 0.357 ALGO for the three-page, two-uint signed claim
(100,000 base + 200,000 for two extra pages + 57,000 for two uints), refundable
on delete.

## Size limits

The signed-claim module uses 3 of the 8 possible pages (EPP 2), well inside both
the free 8,192-byte envelope and the 16,384-byte absolute limit. The e2e test
also asserts that creating it with one fewer extra page is rejected with
`approval program too long`, confirming the page accounting.

A compiler-generated module can exceed 16,384 bytes if a contract pulls in large
dependencies or is built without the size profile; at that point it cannot be
deployed at any page count. For the fixtures measured here, though, the module
is not close to the size ceiling. Setup pricing is what makes byte count and
function count expensive, not the page cap itself.

## Clear-state caveat

The clear-state program receives only the unpooled 700-unit budget, even when
the group has a large pool. Any compiler-generated module costs more than 700
units to set up, so a Rust clear-state program cannot run. ClearState still
closes the account (the failure is ignored), but a Rust CSP is effectively
inert. The experiments use the 40-byte WAT `approve.wasm` as the clear program;
an application with WASM approval and clear programs is still valid because both
carry the WASM magic.

## Verdict

- **Size: fits.** Optimized Rust applications are well inside the page limits.
  4.1 KiB is typical for a library-using contract and needs three pages.
- **Budget: does not fit comfortably.** Setup alone consumes 1.7–4.3K units, so
  every Rust app needs pooling, including for creation. A representative
  one-signature contract needs 12 of 16 group slots to call. Useful contracts
  that do two crypto operations need all 16 slots, and ones that grow the module
  substantially exceed the 11,200-unit pooled ceiling and cannot execute under
  normal limits.
- **Clear state: does not fit** for a Rust-authored CSP.

This is a protocol-level budget issue, not a wasmi throughput issue: the guest
execution and host calls take tens of microseconds to tens of milliseconds,
while the deterministic setup charge is several times one application's
allowance. Options for productionization include a WASM-specific setup allowance
or budget, counting pooled budget differently for setup, or reducing the guest
runtime's stack and module size. The current 700/11,200 model makes simple Rust
apps possible only with pooling and makes crypto-heavy ones impractical.

## Reproduce

```bash
make wasmlib
make wasm-guest-fixtures
make wasm-setupcost                    # exact setup fuel for both fixtures

# Exact costs under a normal 16-call pooled budget (no lifted caps).
WASM_ADVERSARIAL=1 go test ./data/transactions/logic \
  -run '^TestWasmRustContractBudget$' -v -count=1

# Deploy and execute through algod, finding the minimal pooled groups.
make build
TESTDIR=/tmp/wasm-contract-e2e make wasm-contract-e2e

# Same flow through goal on a ConsensusFuture network (representative contract).
TESTFILTER=e2e-wasm-contract-budget.sh ./test/scripts/e2e.sh
```

The e2e test is
`test/e2e-go/features/transactions/wasm_contract_test.go`; the `goal`
reproduction is
`test/scripts/e2e_subs/e2e-wasm-contract-budget.sh`.

## Caveats

- One machine (linux/arm64, warm caches). Costs are deterministic; wall times
  are not part of this measurement.
- The deploy cost measures a create that returns early (`ApplicationID == 0`). A
  contract that initializes state during creation would pay closer to its full
  call cost to deploy.
- The fixed bearer-voucher signature in the `goal` script is a CLI
  simplification; the e2e test derives a real signature from the fixed seed.
- The dummy app calls cost a few units each, so the minimal N can be one larger
  than `ceil(cost / 700)` when the margin is tight. The e2e test reports the
  measured minimum, not the arithmetic estimate.
- Setup is reported from the deterministic pricing harness, not by lifting a
  cap during execution.
