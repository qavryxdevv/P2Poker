//! Schema validation for everything arriving from the network. Never `eval`,
//! and never a self-describing deserialiser that can allocate without bound.
