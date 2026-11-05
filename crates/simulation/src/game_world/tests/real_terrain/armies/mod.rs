//! Army march measurements on carved real-terrain shapes: a river with one
//! off-grid bridge, a forest wall with a five-tile gap, and a one-tile pass.
//! `scene` builds a cell, `march` runs it and reports one table row. The full
//! table is an on-demand measurement, written to the ignored `.cache/armies`.
use super::*;

mod march;
mod mounted;
mod scene;
mod table;
