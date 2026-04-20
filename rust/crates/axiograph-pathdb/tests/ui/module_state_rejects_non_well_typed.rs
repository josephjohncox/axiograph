use axiograph_pathdb::{Accepted, Module, WellTypedModuleState};

fn require_well_typed<S: WellTypedModuleState>(_module: &Module<S>) {}

fn main() {
    let _boundary: fn(&Module<Accepted>) = require_well_typed::<Accepted>;
}
