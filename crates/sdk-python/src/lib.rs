//! Python bindings of the AoeWorld AI SDK. The pure-Python package in
//! `python/aoeworld` re-exports this module and adds asyncio helpers.
mod client;
mod enums;
mod lobby;
mod rules;
mod types;
mod world;

use pyo3::prelude::*;

#[pymodule]
fn _native(py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    enums::register(module)?;
    rules::register(module)?;
    module.add_class::<types::Unit>()?;
    module.add_class::<types::Building>()?;
    module.add_class::<types::ResourceNode>()?;
    module.add_class::<types::Animal>()?;
    module.add_class::<types::Economy>()?;
    module.add_class::<types::Event>()?;
    module.add_class::<world::World>()?;
    module.add_class::<client::Client>()?;
    module.add_class::<client::Fleet>()?;
    module.add_class::<client::Decision>()?;
    module.add_class::<client::RunSummary>()?;
    module.add_class::<lobby::SlotAssignment>()?;
    module.add_class::<lobby::MatchInfo>()?;
    module.add_function(wrap_pyfunction!(client::connect, module)?)?;
    module.add_function(wrap_pyfunction!(lobby::start_match, module)?)?;
    module.add("AoeError", py.get_type::<client::AoeError>())?;
    module.add(
        "ConnectionClosed",
        py.get_type::<client::ConnectionClosed>(),
    )?;
    module.add("Refused", py.get_type::<client::Refused>())?;
    Ok(())
}
