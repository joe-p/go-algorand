#![no_std]

use algokit::GlobalStateKey;
use algokit::Uint256;

const TOTAL_SUPPLY: GlobalStateKey<Uint256> = GlobalStateKey::new(b"ts");

fn get_total_supply() -> Uint256 {
    TOTAL_SUPPLY.get()
}

fn set_total_supply(value: &Uint256) {
    TOTAL_SUPPLY.set(value);
}

#[unsafe(no_mangle)]
pub fn program() -> u64 {
    let amt = Uint256::from(100u64.to_be_bytes().as_slice());
    set_total_supply(&amt);
    let current_supply = get_total_supply();

    let new_supply = current_supply + amt;
    set_total_supply(&new_supply);

    // Return the new total supply as an example

    if get_total_supply() == Uint256::from(100u64.to_be_bytes().as_slice()) {
        1 // Indicating success
    } else {
        0 // Indicating failure
    }
}
