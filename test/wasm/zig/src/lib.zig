const algokit = @import("algokit.zig");
const std = @import("std");

const TOTAL_SUPPLY = algokit.GlobalStateKey(u256).init("ts");

fn get_total_supply() !u256 {
    return try TOTAL_SUPPLY.get();
}

fn set_total_supply(value: u256) void {
    TOTAL_SUPPLY.set(value);
}

export fn program() u64 {
    const amt: u256 = 100;

    set_total_supply(amt);

    const current_supply = get_total_supply() catch return 0;

    const new_supply = current_supply + amt;

    set_total_supply(new_supply);

    const final_supply = get_total_supply() catch return 0;

    if (final_supply == new_supply) {
        return 1; // Success
    }

    return 0; // Failure
}
