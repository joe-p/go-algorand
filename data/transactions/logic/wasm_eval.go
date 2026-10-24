// Copyright (C) 2019-2026 Algorand Foundation Ltd.
// This file is part of go-algorand
//
// go-algorand is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// go-algorand is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with go-algorand.  If not, see <https://www.gnu.org/licenses/>.

package logic

import (
	"errors"

	"github.com/algorand/go-algorand/data/transactions"
)

// wasmEnabled reports whether the protocol runs WASM LogicSigs.
func wasmEnabled(params *EvalParams) bool {
	return params != nil && params.Proto != nil && params.Proto.EnableWasmLogicSig
}

// checkWasmSignature statically checks a WASM LogicSig. It does not run it.
func checkWasmSignature(program []byte, params *EvalParams) error {
	if params == nil || params.Proto == nil || params.Proto.LogicSigVersion == 0 {
		return errLogicSigNotSupported
	}
	if !params.Proto.EnableWasmLogicSig {
		return errors.New("WASM LogicSigs not enabled")
	}
	if err := validateWasmProgram(program); err != nil {
		return err
	}
	return nil
}

// evalWasmSignatureProgram evaluates a WASM LogicSig authorizing the gi-th
// transaction in params. It mirrors EvalSignatureProgram, but runs the wasm
// executor instead of the AVM. The budget of the AVM is reused as the fuel
// limit, one fuel unit to one cost unit.
func evalWasmSignatureProgram(program []byte, args [][]byte, gi int, params *EvalParams) (bool, *EvalContext, error) {
	cx := EvalContext{
		EvalParams: params,
		runMode:    ModeSig,
		groupIndex: gi,
		txn:        &params.TxnGroup[gi],
		sigArgs:    args,
	}
	// A WASM module has no AVM version byte. Use the protocol's maximum so the
	// `txn` field version gates match what the AVM would allow.
	cx.version = params.Proto.LogicSigVersion
	// Save scratch, matching the AVM path. WASM cannot access it yet, but
	// error returns and tracers might.
	cx.pastScratch[cx.groupIndex] = &cx.Scratch

	pass, err := wasmEval(&cx, program)
	if err != nil {
		err = cx.evalError(err)
	}
	return pass, &cx, err
}

// wasmEval runs program in cx, charging the fuel it consumes as AVM cost and
// recording which arguments it read.
func wasmEval(cx *EvalContext, program []byte) (bool, error) {
	// The same argument bounds the AVM path enforces.
	if len(cx.sigArgs) > transactions.EvalMaxArgs {
		return false, errTooManyArgs
	}
	for _, arg := range cx.sigArgs {
		if len(arg) > transactions.MaxLogicSigArgSize {
			return false, errLogicSigArgTooLarge
		}
	}

	budget := cx.remainingBudget()
	if budget < 0 {
		budget = 0
	}

	pass, fuelUsed, accessed, err := runWasmProgram(program, cx.sigArgs, uint64(budget), cx)
	if err != nil {
		return false, err
	}

	cost := int(fuelUsed)
	if cost > budget {
		cost = budget
	}
	cx.cost += cost
	if cx.PooledLogicSigBudget != nil {
		*cx.PooledLogicSigBudget -= cost
	}

	for i, word := range accessed {
		if i < len(cx.argsRead) {
			cx.argsRead[i] = word
		}
	}

	return pass, nil
}
