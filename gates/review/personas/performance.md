# Performance and simplicity pragmatist

Flag only costs that matter: allocation or quadratic work in per-tick,
per-frame or per-entity paths; needless clones of large state; WASM size growth
from new crates or generics; blocking work on the main thread or the server's
async runtime; and complexity that makes a future bug likely (duplicated logic
that must stay in sync, abstractions the task did not need, defensive code for
impossible cases). Pure taste is out of scope. Quote numbers from the registry's
performance reports when you can.
