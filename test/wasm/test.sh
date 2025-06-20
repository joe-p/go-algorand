#!/bin/bash

# NOTE: We use iterations instead of time because we manually control the timers
gotestsum --format=standard-verbose -- -v -race -count=1 /Users/joe/git/algorand/go-algorand/data/transactions/logic -run ^TestWasm -benchmem -bench WasmLoop -benchtime 1000x
