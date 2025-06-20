#!/bin/bash

./build.sh && gotestsum --format=standard-verbose -- -v -race -count=1 /Users/joe/git/algorand/go-algorand/data/transactions/logic -run ^TestWasm
