#!/bin/bash

gotestsum --format=standard-verbose -- -v -race -count=1 /Users/joe/git/algorand/go-algorand/data/transactions/logic -run ^TestWasm -benchmem -bench WasmLoop -benchtime 10000x
