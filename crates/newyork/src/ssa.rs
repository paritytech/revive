//! SSA conversion utilities for the newyork IR.
//!
//! This module provides the SSA builder that tracks variable definitions
//! and handles phi-node insertion for control flow joins.

use crate::ir::{Type, Value, ValueId};
use std::collections::BTreeMap;

/// SSA builder that tracks variable definitions and creates fresh value IDs.
#[derive(Debug)]
pub struct SsaBuilder {
    /// Next value ID to allocate.
    next_value_id: u32,
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
            next_value_id: 0,
            current_scope: BTreeMap::new(),
            scope_stack: Vec::new(),
        }
    }

    /// Allocates a fresh value ID.
    pub fn fresh_value(&mut self) -> ValueId {
        let id = ValueId::new(self.next_value_id);
        self.next_value_id += 1;
        id
    }

    /// Allocates a fresh typed value with the given type.
    pub fn fresh_typed_value(&mut self, ty: Type) -> Value {
        Value::new(self.fresh_value(), ty)
    }

    /// Defines a variable with the given name and value.
    pub fn define(&mut self, name: &str, value: Value) {
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
        self.current_scope = self.scope_stack.pop().unwrap_or_default();
        exited
    }

    /// Gets the current scope (for computing modified variables).
    pub fn current_scope(&self) -> &BTreeMap<String, Value> {
        &self.current_scope
    }

    /// Gets the parent scope (if any) for comparison.
    pub fn parent_scope(&self) -> Option<&BTreeMap<String, Value>> {
        self.scope_stack.last()
    }

    /// Computes variables that were modified in the current scope compared to parent.
    pub fn modified_variables(&self) -> Vec<(String, Value, Value)> {
        let Some(parent) = self.parent_scope() else {
            return Vec::new();
        };

        let mut modified = Vec::new();
        for (name, &current_value) in &self.current_scope {
            if let Some(&parent_value) = parent.get(name) {
                if current_value.id != parent_value.id {
                    modified.push((name.clone(), parent_value, current_value));
                }
            }
        }
        modified
    }

    /// Merges two scopes at a control flow join point.
    /// Returns the list of (variable_name, phi_result, then_value, else_value).
    pub fn merge_scopes(
        &mut self,
        then_scope: &BTreeMap<String, Value>,
        else_scope: &BTreeMap<String, Value>,
    ) -> Vec<(String, ValueId, Value, Value)> {
        let mut merges = Vec::new();

        // Find variables that differ between branches
        for (name, &then_value) in then_scope {
            if let Some(&else_value) = else_scope.get(name) {
                if then_value.id != else_value.id {
                    // Need a phi node
                    let phi_result = self.fresh_value();
                    merges.push((name.clone(), phi_result, then_value, else_value));

                    // Update current scope with merged value
                    self.define(name, Value::new(phi_result, then_value.ty));
                } else {
                    // Same value, just propagate
                    self.define(name, then_value);
                }
            } else {
                // Variable only in then branch - propagate
                self.define(name, then_value);
            }
        }

        // Variables only in else branch
        for (name, &else_value) in else_scope {
            if !then_scope.contains_key(name) {
                self.define(name, else_value);
            }
        }

        merges
    }

    /// Restores scope from a saved state, used after control flow constructs.
    pub fn restore_scope(&mut self, scope: BTreeMap<String, Value>) {
        self.current_scope = scope;
    }

    /// Gets the next value ID that will be allocated (for planning).
    pub fn peek_next_id(&self) -> u32 {
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
        let v0 = builder.fresh_value();
        let v1 = builder.fresh_value();
        assert_eq!(v0.0, 0);
        assert_eq!(v1.0, 1);
    }

    #[test]
    fn test_define_and_lookup() {
        let mut builder = SsaBuilder::new();
        let v = builder.fresh_typed_value(Type::Int(BitWidth::I256));
        builder.define("x", v);
        assert_eq!(builder.lookup("x"), Some(v));
        assert_eq!(builder.lookup("y"), None);
    }

    #[test]
    fn test_nested_scopes() {
        let mut builder = SsaBuilder::new();
        let v0 = builder.fresh_typed_value(Type::Int(BitWidth::I256));
        builder.define("x", v0);

        builder.enter_scope();
        let v1 = builder.fresh_typed_value(Type::Int(BitWidth::I256));
        builder.define("x", v1);
        assert_eq!(builder.lookup("x"), Some(v1));

        builder.exit_scope();
        assert_eq!(builder.lookup("x"), Some(v0));
    }
}
