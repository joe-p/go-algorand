# Spike: `wasm_eval`

This branch adds a new opcode: `wasm_eval` . It currently takes one immediate which is the WASM bytecode. I am using an immediate because it allows us to avoid the 4k stack limit and perhaps more importantly it means that app logic updates cannot happen dynamically (must use `UpdateApplication` and increase the version). Eventually I want to also pass another immediate that defines a gas limit.

`wasm_eval` should have an upfront cost of ~13x an app call (9,100). This number is based on the benchmarks below.

## Benchmarks

| Benchmark                           | Program                                                      | Rationale                                                                                    | Takeaway                                                            |
| ----------------------------------- | ------------------------------------------------------------ | -------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| BenchmarkAppFibo19                  | Pure AVM recursive Fibonacci of 19                           | This is the highest number we can run in the algorithm with the max opcode budget (256\*700) | This is the worst-case scenario for AVM program execution time      |
| BenchmarkAppWasmFibo19              | WASM recursive Fibonacci of 19                               | Algorithm that matches what the AVM currently maxes out on                                   | WASM is **8x faster** than AVM                                      |
| BenchmarkAppWasmInt1Repeated20Times | 20 `wasm_eval` calls of a WASM program that simply returns 1 | We want to see how many WASM cold starts we can fit in the time of today's max compute       | 20 WASM inits ~= 256 app budgets, thus 1 WASM init ~= 13 app budget |

```
❯ go test -v -count=1 ./ledger -run ^$ -bench "(Fibo19|Repeat)"
goos: linux
goarch: arm64
pkg: github.com/algorand/go-algorand/ledger
BenchmarkAppFibo19
    ledger_perf_test.go:340: built 1 blocks, each with 32233 txns
BenchmarkAppFibo19-8                                   1        88721556310 ns/op
BenchmarkAppWasmFibo19
    ledger_perf_test.go:340: built 1 blocks, each with 32233 txns
BenchmarkAppWasmFibo19-8                               1        10974437348 ns/op
BenchmarkAppWasmInt1Repeated20Times
    ledger_perf_test.go:340: built 1 blocks, each with 32233 txns
BenchmarkAppWasmInt1Repeated20Times-8                  1        82588779486 ns/op
PASS
ok      github.com/algorand/go-algorand/ledger  364.534s
```

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
