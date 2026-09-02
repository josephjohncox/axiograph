//! Structured-data schema discovery for Axiograph.
//!
//! SQL DDL and JSON samples are untrusted inputs. These adapters infer
//! proposal-oriented schema descriptions; they do not emit or accept canonical
//! `.axi` meaning.

pub mod json;
pub mod sql;
