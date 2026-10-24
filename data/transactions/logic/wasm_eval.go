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

	"github.com/algorand/go-algorand/data/basics"
	"github.com/algorand/go-algorand/data/transactions"
)

// wasmFuelPerCost converts AVM budget units into wasmi fuel. The Rust meter
// uses raw fuel only: guest, host, and setup charges remain unchanged when this
// exchange rate changes. Host operations are calibrated toward 1 ns/fuel.
const wasmFuelPerCost uint64 = 10

// wasmEnabled reports whether the protocol runs WASM LogicSigs.
func wasmEnabled(params *EvalParams) bool {
	return params != nil && params.Proto != nil && params.Proto.EnableWasmLogicSig
}

// wasmAppEnabled reports whether the protocol runs WASM application programs.
func wasmAppEnabled(params *EvalParams) bool {
	return params != nil && params.Proto != nil && params.Proto.EnableWasmApp
}

// innerProgramVersion returns the effective AVM version to use in inner
// app-call checks. A WASM program carries no AVM version byte, so it is treated
// as the protocol's maximum AVM version, matching how WASM programs are gated on
// the direct evaluation path. Otherwise it is the program's own version byte.
func innerProgramVersion(cx *EvalContext, program []byte) (uint64, error) {
	if wasmAppEnabled(cx.EvalParams) && IsWasmProgram(program) {
		return cx.Proto.LogicSigVersion, nil
	}
	version, _, err := transactions.ProgramVersion(program)
	return version, err
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

// checkWasmContract statically checks a WASM application program. It does not
// run it. It mirrors checkWasmSignature, but is gated on EnableWasmApp.
func checkWasmContract(program []byte, params *EvalParams) error {
	if params == nil || params.Proto == nil || params.Proto.LogicSigVersion == 0 {
		return errLogicSigNotSupported
	}
	if !params.Proto.EnableWasmApp {
		return errors.New("WASM app programs not enabled")
	}
	if err := validateWasmProgram(program); err != nil {
		return err
	}
	return nil
}

// evalWasmSignatureProgram evaluates a WASM LogicSig authorizing the gi-th
// transaction in params. It mirrors EvalSignatureProgram, but runs the wasm
// executor instead of the AVM, with wasmFuelPerCost fuel per AVM cost unit.
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

// wasmEvalContract runs a WASM application program (approval or clear-state) in
// cx. It sets up the same context state that eval() would for the AVM before
// running: the version gates and the EvalDelta maps. It is called by
// EvalContract in place of eval().
func wasmEvalContract(program []byte, cx *EvalContext) (bool, error) {
	// A WASM module has no AVM version byte. Use the protocol's maximum so the
	// txn field version gates match what the AVM would allow.
	cx.version = cx.Proto.LogicSigVersion
	// eval() resets the delta maps at the start of every run. Mirror that so the
	// app call leaves a consistent (empty) delta even though WASM cannot touch
	// state yet.
	cx.txn.EvalDelta.GlobalDelta = basics.StateDelta{}
	cx.txn.EvalDelta.LocalDeltas = make(map[uint64]basics.StateDelta)

	return wasmEval(cx, program)
}

// wasmEval runs program in cx, converting consumed fuel to AVM cost. In
// signature mode it validates the LogicSig argument bounds and records which
// arguments were read; in application mode the guest reads arguments through
// `txna` instead, so those signature-only rules do not apply.
func wasmEval(cx *EvalContext, program []byte) (bool, error) {
	if cx.runMode == ModeSig {
		// The same argument bounds the AVM path enforces.
		if len(cx.sigArgs) > transactions.EvalMaxArgs {
			return false, errTooManyArgs
		}
		for _, arg := range cx.sigArgs {
			if len(arg) > transactions.MaxLogicSigArgSize {
				return false, errLogicSigArgTooLarge
			}
		}
	}

	budget := cx.remainingBudget()
	if budget < 0 {
		budget = 0
	}

	if uint64(budget) > ^uint64(0)/wasmFuelPerCost {
		return false, errors.New("WASM fuel budget overflow")
	}
	pass, fuelUsed, accessed, err := runWasmProgram(program, cx.sigArgs, uint64(budget)*wasmFuelPerCost, cx)

	cx.wasmFuelUsed = fuelUsed
	// Round up so even a partial cost unit is charged to the shared budget.
	cost := int(fuelUsed / wasmFuelPerCost)
	if fuelUsed%wasmFuelPerCost != 0 {
		cost++
	}
	if cost > budget {
		cost = budget
	}
	cx.cost += cost
	// At most one of these pooled budgets is non-nil, matching the AVM's step().
	switch {
	case cx.PooledApplicationBudget != nil:
		*cx.PooledApplicationBudget -= cost
	case cx.PooledLogicSigBudget != nil:
		*cx.PooledLogicSigBudget -= cost
	}
	// Charge even failed evaluations: clear-state errors can be ignored, but
	// compilation, allocation, and any execution already performed are not free.
	if err != nil {
		return false, err
	}

	if cx.runMode == ModeSig {
		for i, word := range accessed {
			if i < len(cx.argsRead) {
				cx.argsRead[i] = word
			}
		}
	}

	return pass, nil
}
