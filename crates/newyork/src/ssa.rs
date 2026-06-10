//! SSA conversion utilities for the newyork IR.
//!
//! This module provides the SSA builder that allocates fresh value IDs and tracks
//! variable definitions across lexical scopes. Control-flow joins — choosing which
//! values cross `if`/`for`/`switch` boundaries — are wired up by the translator in
//! `from_yul.rs`, and the corresponding PHI nodes are emitted only later, in the
//! LLVM IR (`to_llvm.rs`); this module does not create PHI nodes itself.

use crate::ir::{Type, Value, ValueId};
use std::collections::BTreeMap;

/// SSA builder that tracks variable definitions and creates fresh value IDs.
#[derive(Debug)]
pub struct SsaBuilder {
    /// Next value ID to allocate.
    next_value_id: ValueId,
    /// Current scope's variable bindings: variable name -> SSA value.
    current_scope: BTreeMap<String, Value>,
    /// Stack of saved scopes for nested blocks.
    scope_stack: Vec<BTreeMap<String, Value>>,
}

impl Default for SsaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SsaBuilder {
    /// Creates a new SSA builder.
    pub fn new() -> Self {
        SsaBuilder {
            next_value_id: ValueId(0),
            current_scope: BTreeMap::new(),
            scope_stack: Vec::new(),
        }
    }

    /// Allocates a fresh value ID.
    pub fn fresh_id(&mut self) -> ValueId {
        self.next_value_id.fresh()
    }

    /// Allocates a fresh typed value with the given type.
    pub fn fresh_typed_value(&mut self, value_type: Type) -> Value {
        Value::new(self.fresh_id(), value_type)
    }

    /// Declares a new variable in the current scope.
    ///
    /// Panics if a binding for `name` already exists in the current scope.
    pub fn declare(&mut self, name: &str, value: Value) {
        assert!(
            !self.current_scope.contains_key(name),
            "ICE: SsaBuilder::declare called for already-declared variable `{name}`",
        );
        self.current_scope.insert(name.to_string(), value);
    }

    /// Assigns a new value to an existing variable in the current scope.
    ///
    /// Panics if no binding for `name` exists in the current scope.
    pub fn assign(&mut self, name: &str, value: Value) {
        assert!(
            self.current_scope.contains_key(name),
            "ICE: SsaBuilder::assign called for undeclared variable `{name}`",
        );
        self.current_scope.insert(name.to_string(), value);
    }

    /// Looks up a variable by name, returning its current SSA value.
    pub fn lookup(&self, name: &str) -> Option<Value> {
        self.current_scope.get(name).copied()
    }

    /// Enters a new scope, saving the current scope.
    pub fn enter_scope(&mut self) {
        self.scope_stack.push(self.current_scope.clone());
    }

    /// Exits the current scope, restoring the previous scope.
    /// Returns the scope that was exited (for computing modified variables).
    pub fn exit_scope(&mut self) -> BTreeMap<String, Value> {
        let exited = std::mem::take(&mut self.current_scope);
        self.current_scope = self.scope_stack.pop().unwrap_or_else(|| {
            panic!(
                "ICE: SsaBuilder::exit_scope called without a matching enter_scope; \
                 {} binding(s) about to be silently dropped: {:?}",
                exited.len(),
                exited.keys().collect::<Vec<_>>(),
            )
        });
        exited
    }

    /// Gets the current scope (for computing modified variables).
    pub fn current_scope(&self) -> &BTreeMap<String, Value> {
        &self.current_scope
    }

    /// Restores scope from a saved state, used after control flow constructs.
    pub fn restore_scope(&mut self, scope: BTreeMap<String, Value>) {
        self.current_scope = scope;
    }

    /// Gets the next value ID that will be allocated (for planning).
    pub fn peek_next_id(&self) -> ValueId {
        self.next_value_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::BitWidth;

    #[test]
    fn test_fresh_values() {
        let mut builder = SsaBuilder::new();
        let v0 = builder.fresh_id();
        let v1 = builder.fresh_id();
        assert_eq!(v0.0, 0);
        assert_eq!(v1.0, 1);
    }

    #[test]
    fn test_declare_and_lookup() {
        let mut builder = SsaBuilder::new();
        let v = builder.fresh_typed_value(Type::Int(BitWidth::I256));
        builder.declare("x", v);
        assert_eq!(builder.lookup("x"), Some(v));
        assert_eq!(builder.lookup("y"), None);
    }

    #[test]
    fn test_nested_scopes() {
        let mut builder = SsaBuilder::new();
        let v0 = builder.fresh_typed_value(Type::Int(BitWidth::I256));
        builder.declare("x", v0);

        builder.enter_scope();
        let v1 = builder.fresh_typed_value(Type::Int(BitWidth::I256));
        builder.assign("x", v1);
        assert_eq!(builder.lookup("x"), Some(v1));

        builder.exit_scope();
        assert_eq!(builder.lookup("x"), Some(v0));
    }

    #[test]
    #[should_panic(expected = "ICE: SsaBuilder::declare called for already-declared variable")]
    fn declare_twice_panics() {
        let mut builder = SsaBuilder::new();
        let v = builder.fresh_typed_value(Type::Int(BitWidth::I256));
        builder.declare("x", v);
        builder.declare("x", v);
    }

    #[test]
    #[should_panic(expected = "ICE: SsaBuilder::assign called for undeclared variable")]
    fn assign_undeclared_panics() {
        let mut builder = SsaBuilder::new();
        let v = builder.fresh_typed_value(Type::Int(BitWidth::I256));
        builder.assign("x", v);
    }
}
