#!/bin/bash

# Reproduce the compiler-generated WASM contract size/budget measurement with
# the goal CLI on a ConsensusFuture network.
#
# The WASM approval program executes during creation too, so deployment itself
# needs a pooled group. Defaults are the measured minimums from
# WASM_CONTRACTS.md; override DEPLOY_CALLS/CALL_CALLS if the fixtures change.

filename=$(basename "$0")
scriptname="${filename%.*}"
date "+${scriptname} start %Y%m%d_%H%M%S"
# TIMEOUT=600

set -e
set -x
set -o pipefail
export SHELLOPTS

WALLET=$1

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
WASM_DIR="${DIR}/../../../data/transactions/logic/testdata/wasm"
CONTRACT="${WASM_DIR}/app_rust_signed_claim.wasm"
CLEAR="${WASM_DIR}/approve.wasm"

DEPLOY_CALLS=${DEPLOY_CALLS:-8}
CALL_CALLS=${CALL_CALLS:-13}

gcmd="goal -w ${WALLET}"
ACCOUNT=$(${gcmd} account list | awk '{ print $3 }')

SIZE=$(wc -c < "${CONTRACT}")
PAGES=$(( (SIZE + 2047) / 2048 ))
EPP=$(( PAGES - 1 ))

# The issuer signature is a fixed bearer voucher over the 8-byte big-endian
# amount 1000. goal cannot sign arbitrary bytes, so the constant is embedded
# here; it matches the key compiled into the Rust fixture.
SIG_B64="+oCY8b4okJBmNnnmRYtXwb/5UVuEvLOWchNjggV2aufq0KFiLoy27//4/vd4BTbZx7/pGTMJhNm6dN+i2igSDQ=="

T="${TEMPDIR}/wasm-contract"
mkdir -p "${T}"

# sim_field prints the group failure message, added budget, consumed budget, and
# first-transaction consumed budget from a goal simulate result.
sim_result() {
    python3 - "$1" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))
g = r["txn-groups"][0]
fail = g.get("failure-message") or ""
txns = g.get("txn-results", [])
first = txns[0].get("app-budget-consumed", 0) if txns else 0
print(f"{fail}|{g.get('app-budget-added', 0)}|{g.get('app-budget-consumed', 0)}|{first}")
PY
}

# build_group MODE CALLS [APPID]
# Writes wg-group.stx: one create/call transaction followed by cheap no-op calls
# to the dummy app. Only app calls contribute to the pooled budget.
build_group() {
    local mode=$1
    local calls=$2
    local appid=${3:-}
    rm -f "${T}"/wg-*.tx "${T}"/wg-group.stx

    if [ "${mode}" = "create" ]; then
        ${gcmd} app create --creator "${ACCOUNT}" \
            --approval-prog-raw "${CONTRACT}" --clear-prog-raw "${CLEAR}" \
            --extra-pages "${EPP}" --global-ints 4 --global-byteslices 0 \
            --local-ints 0 --local-byteslices 0 \
            --out "${T}/wg-0.tx" --note "wasm-deploy"
    else
        ${gcmd} app call --app-id "${appid}" --from "${ACCOUNT}" \
            --app-arg "b64:${SIG_B64}" --app-arg "int:1000" \
            --out "${T}/wg-0.tx" --note "wasm-call"
    fi
    local i=1
    while [ "${i}" -lt "${calls}" ]; do
        ${gcmd} app call --app-id "${DUMMY}" --from "${ACCOUNT}" \
            --out "${T}/wg-${i}.tx" --note "wasm-pool-${i}"
        i=$((i + 1))
    done
    local files=()
    for i in $(seq 0 $((calls - 1))); do
        files+=("${T}/wg-${i}.tx")
    done
    cat "${files[@]}" | ${gcmd} clerk group -i - -o "${T}/wg-group.tx"
    ${gcmd} clerk sign -i "${T}/wg-group.tx" -o "${T}/wg-group.stx"
}

simulate_group() {
    rm -f "${T}/sim.json"
    ${gcmd} clerk simulate -t "${T}/wg-group.stx" -o "${T}/sim.json" >/dev/null 2>&1 || true
    [ -f "${T}/sim.json" ] || { echo "simulate produced no result"; false; }
    sim_result "${T}/sim.json"
}

# Dummy AVM app: cheap budget grants.
printf '#pragma version 8\nint 1\n' > "${T}/int1.teal"
DUMMY=$(${gcmd} app create --creator "${ACCOUNT}" \
    --approval-prog "${T}/int1.teal" --clear-prog "${T}/int1.teal" \
    --global-ints 0 --global-byteslices 0 --local-ints 0 --local-byteslices 0 \
    | grep Created | awk '{ print $6 }')

date "+${scriptname} contract=${SIZE}B pages=${PAGES} extra-pages=${EPP} %Y%m%d_%H%M%S"

# Deploy: the approval program runs during creation, so it needs a pooled group.
build_group create "${DEPLOY_CALLS}"
IFS='|' read -r FAIL ADDED CONSUMED FIRST < <(simulate_group)
[ -z "${FAIL}" ] || { echo "deploy simulate failed: ${FAIL}"; false; }
echo "deploy: calls=${DEPLOY_CALLS} budget-added=${ADDED} budget-consumed=${CONSUMED} first-txn-cost=${FIRST}"
${gcmd} clerk rawsend -f "${T}/wg-group.stx"

APPID=$(${gcmd} account info -a "${ACCOUNT}" \
    | sed -n '/Created Apps:/,/Opted In Apps:/p' \
    | grep -o 'ID [0-9]*' | awk '{ print $2 }' | sort -n | tail -1)
[ -n "${APPID}" ] || { echo "could not find created app id"; false; }
echo "deployed app=${APPID}"

APPADDR=$(${gcmd} app info --app-id "${APPID}" | grep "Application account:" | awk '{ print $3 }')
${gcmd} clerk send --from "${ACCOUNT}" --to "${APPADDR}" --amount 1000000

# A single call has 700 units and cannot pay the setup charge.
build_group call 1 "${APPID}"
IFS='|' read -r FAIL ADDED CONSUMED FIRST < <(simulate_group)
[ -n "${FAIL}" ] || { echo "single call unexpectedly succeeded"; false; }
echo "single call failed as expected: ${FAIL}"

# The measured minimum pooled group must succeed.
build_group call "${CALL_CALLS}" "${APPID}"
IFS='|' read -r FAIL ADDED CONSUMED FIRST < <(simulate_group)
[ -z "${FAIL}" ] || { echo "pooled call simulate failed: ${FAIL}"; false; }
echo "call: calls=${CALL_CALLS} budget-added=${ADDED} budget-consumed=${CONSUMED} first-txn-cost=${FIRST}"
${gcmd} clerk rawsend -f "${T}/wg-group.stx"

# One fewer call must fail.
if [ "${CALL_CALLS}" -gt 1 ]; then
    build_group call $((CALL_CALLS - 1)) "${APPID}"
    IFS='|' read -r FAIL ADDED CONSUMED FIRST < <(simulate_group)
    [ -n "${FAIL}" ] || { echo "smaller pooled group unexpectedly succeeded"; false; }
    echo "one fewer call failed as expected"
fi

# The same sender cannot claim twice.
build_group call "${CALL_CALLS}" "${APPID}"
IFS='|' read -r FAIL ADDED CONSUMED FIRST < <(simulate_group)
[ -n "${FAIL}" ] || { echo "replay unexpectedly succeeded"; false; }
echo "replay rejected: ${FAIL}"

READ=$(${gcmd} app read --app-id "${APPID}" --global --guess-format)
echo "${READ}"
[[ "${READ}" == *"1000"* ]] || { echo "unexpected global state: ${READ}"; false; }

date "+${scriptname} OK %Y%m%d_%H%M%S"
