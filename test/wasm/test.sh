#!/bin/bash

# NOTE: We use iterations instead of time because we manually control the timers
# go test -v -race -count=1 /Users/joe/git/algorand/go-algorand/data/transactions/logic /Users/joe/git/algorand/go-algorand/ledger -run Wasm -benchmem -bench Wasm -benchtime 1000x
go test -v -race -count=1  /Users/joe/git/algorand/go-algorand/ledger -run Wasm   -benchmem -bench Wasm -benchtime 500x
