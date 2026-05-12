# Spike: `wasm_eval`

This branch adds a new opcode: `wasm_eval` . It currently takes one immediate which is the WASM bytecode. I am using an immediate because it allows us to avoid the 4k stack limit and perhaps more importantly it means that app logic updates cannot happen dynamically (must use `UpdateApplication` and increase the version). Eventually I want to also pass another immediate that defines a gas limit.

`wasm_eval` should have an upfront cost of ~13x an app call (9,100). This number is based on the benchmarks below.

## Benchmarks

### Raw Results

```
❯ go test -v -count=1 ./ledger -run ^$ -bench "(RepeatAdd|WasmInt1|Fibo19)"
goos: linux
goarch: arm64
pkg: github.com/algorand/go-algorand/ledger
BenchmarkAppWasmInt1
    ledger_perf_test.go:340: built 1 blocks, each with 32233 txns
BenchmarkAppWasmInt1-8                         1        5009875815 ns/op
BenchmarkAppFibo19AppBudget256X
    ledger_perf_test.go:340: built 1 blocks, each with 32233 txns
BenchmarkAppFibo19AppBudget256X-8              1        89524677072 ns/op
BenchmarkAppRepeatAddBudget13X
    ledger_perf_test.go:340: built 1 blocks, each with 32233 txns
BenchmarkAppRepeatAddBudget13X-8               1        5237368123 ns/op
BenchmarkAppWasmFibo19
    ledger_perf_test.go:340: built 1 blocks, each with 32233 txns
BenchmarkAppWasmFibo19-8                       1        12264464731 ns/op
PASS
ok      github.com/algorand/go-algorand/ledger  223.711s
```

### Results Table

| Benchmark          | App Budget Mult | Time (ns/op)   | Time (sec) |
| ------------------ | --------------- | -------------- | ---------- |
| `AVM Repeated Add` | 13x             | 5,237,368,123  | ~5.24      |
| `WASM Int 1`       | N/A (WASM)      | 5,009,875,815  | ~5.01      |
| `AVM Fibo(19)`     | 256x            | 89,524,677,072 | ~89.52     |
| `WASM Fibo(19)`    | N/A (WASM)      | 12,264,464,731 | ~12.26     |

### Key Takeaways

- A `wasm_eval` of a no-op program takes as much time as a 13x app budget AVM app (repeated addition)
- A `wasm_eval` of recursive `fibo(19)` is *7x faster* than AVM recursive `fibo(19)`

### Conclusion

`wasm_eval` has a *slower startup* time than AVM but *faster execution*

## Open Questions

- Most important ones are related to practicality of integration
  - Are we okay with adding a new dependency (wasm-micro-runtime) for contract eval?
  - Are we okay with introducing another language into the go-algorand build process?
    - We could potentially just include prebuilt binaries from a separate repo
- Are there more important benchmarks we should be running? I chose full block perf test since it seems the most holistic but there may be more insightful tests we can run

## Future Work

If we want to go forward with this work at some point in the future, we need to:

- Assign gas costs to WASM ops (WAMR allows gas limiting, we just need to assign costs)
- Do way more testing
  - Particularly across different platforms/hardware
  - Also need to test innertxn performance/implications
- Implement a less "spikey" integration
