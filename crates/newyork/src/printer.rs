//! Pretty printer for the newyork IR.
//!
//! Outputs Yul-like syntax for debugging and inspection. The output is not
//! valid Yul but is designed to be human-readable and clearly show the SSA
//! structure with explicit value IDs.
//!
//! # Example Output
//!
//! ```text
//! object "Test" {
//!     code {
//!         let v0 := 0x00
//!         let v1 := calldataload(v0)
//!         mstore(v0, v1)
//!     }
//!
//!     function add_one(v2: i256) -> (v3: i256) {
//!         let v4 := 0x01
//!         let v3 := add(v2, v4)
//!     }
//! }
//! ```
//!
//! When type-inference results are attached via [`Printer::set_type_info`],
//! value ids whose inferred width is narrower than the i256 default are
//! annotated with that width, e.g. `let v4: i64 := 0x01`. Without inference,
//! bindings carry no annotation because they store no type of their own.

use crate::ir::{
    AddressSpace, BinaryOperation, BitWidth, Block, CallKind, CreateKind, Expression, Function,
    FunctionId, MemoryRegion, Object, Region, Statement, SwitchCase, Type, UnaryOperation, Value,
    ValueId,
};
use crate::type_inference::TypeInference;
use std::collections::BTreeMap;
use std::fmt::{self, Write};

/// Configuration for the IR printer.
#[derive(Clone, Debug)]
pub struct PrinterConfig {
    /// Number of spaces per indentation level.
    pub indent_size: usize,
    /// Whether to print type annotations.
    pub show_types: bool,
    /// Whether to print memory region annotations.
    pub show_regions: bool,
    /// Whether to print static slot annotations for storage operations.
    pub show_static_slots: bool,
}

impl Default for PrinterConfig {
    fn default() -> Self {
        PrinterConfig {
            indent_size: 4,
            show_types: true,
            show_regions: true,
            show_static_slots: true,
        }
    }
}

/// Pretty printer for newyork IR.
pub struct Printer<'a> {
    config: PrinterConfig,
    /// Output buffer.
    output: String,
    /// Current indentation level.
    indent: usize,
    /// Function name lookup for printing calls.
    function_names: BTreeMap<FunctionId, &'a str>,
    /// Optional type-inference results. When present, value ids are annotated
    /// with their post-narrow effective width instead of the statically
    /// assigned i256 default stored in the IR.
    inferred_types: Option<&'a TypeInference>,
}

impl<'a> Printer<'a> {
    /// Creates a new printer with default configuration.
    pub fn new() -> Self {
        Printer {
            config: PrinterConfig::default(),
            output: String::new(),
            indent: 0,
            function_names: BTreeMap::new(),
            inferred_types: None,
        }
    }

    /// Creates a new printer with the given configuration.
    pub fn with_config(config: PrinterConfig) -> Self {
        Printer {
            config,
            output: String::new(),
            indent: 0,
            function_names: BTreeMap::new(),
            inferred_types: None,
        }
    }

    /// Attaches type-inference results so value annotations reflect the
    /// post-narrow effective widths computed by the type-inference pass rather
    /// than the statically assigned i256 defaults stored in the IR.
    pub fn set_type_info(&mut self, type_info: &'a TypeInference) {
        self.inferred_types = Some(type_info);
    }

    /// Prints an IR object and returns the formatted string.
    pub fn print_object(&mut self, object: &'a Object) -> String {
        self.output.clear();
        self.indent = 0;
        self.function_names.clear();
        self.write_object(object);
        std::mem::take(&mut self.output)
    }

    /// Prints an IR function and returns the formatted string.
    pub fn print_function(&mut self, function: &'a Function) -> String {
        self.output.clear();
        self.indent = 0;
        self.write_function(function);
        std::mem::take(&mut self.output)
    }

    /// Prints an IR statement and returns the formatted string.
    pub fn print_statement(&mut self, statement: &Statement) -> String {
        self.output.clear();
        self.indent = 0;
        self.write_statement(statement);
        std::mem::take(&mut self.output)
    }

    /// Prints an IR expression and returns the formatted string.
    pub fn print_expression(&mut self, expression: &Expression) -> String {
        self.output.clear();
        self.write_expression(expression);
        std::mem::take(&mut self.output)
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent * self.config.indent_size {
            self.output.push(' ');
        }
    }

    fn write_newline(&mut self) {
        self.output.push('\n');
    }

    fn write_object(&mut self, object: &'a Object) {
        // FunctionIds are per-object (see ir::FunctionId docs); swap in this
        // object's name table for the duration of the walk and restore on exit
        // so subobjects don't print the parent's names or fall through to
        // `func_<id>` on collisions.
        let saved_function_names = std::mem::take(&mut self.function_names);
        for (id, function) in &object.functions {
            self.function_names.insert(*id, &function.name);
        }

        self.write_indent();
        let _ = write!(self.output, "object \"{}\" {{", object.name);
        self.write_newline();

        self.indent += 1;

        self.write_indent();
        self.output.push_str("code {");
        self.write_newline();

        self.indent += 1;
        self.write_block(&object.code);
        self.indent -= 1;

        self.write_indent();
        self.output.push('}');
        self.write_newline();

        for function in object.functions.values() {
            self.write_newline();
            self.write_function(function);
        }

        for (name, data) in &object.data {
            self.write_newline();
            self.write_indent();
            let _ = write!(self.output, "data \"{}\" hex\"", name);
            for byte in data {
                let _ = write!(self.output, "{:02x}", byte);
            }
            self.output.push('"');
            self.write_newline();
        }

        for (index, subobject) in object.subobjects.iter().enumerate() {
            self.write_newline();
            // Subobjects have their own ValueId namespace and a matching
            // `TypeInference` in `sub_inferences` (same order as `subobjects`,
            // mirroring codegen). Swap to the child's inference for the duration
            // of the walk so subobject value ids aren't looked up in the parent
            // map — where they'd be absent and default to i1. Restore on exit.
            let saved_inferred_types = self.inferred_types;
            self.inferred_types = self
                .inferred_types
                .and_then(|inference| inference.sub_inferences.get(index));
            self.write_object(subobject);
            self.inferred_types = saved_inferred_types;
        }

        self.indent -= 1;
        self.write_indent();
        self.output.push('}');
        self.write_newline();

        self.function_names = saved_function_names;
    }

    fn write_function(&mut self, function: &Function) {
        self.write_indent();
        let _ = write!(self.output, "function {}(", function.name);

        for (i, (id, value_type)) in function.parameters.iter().enumerate() {
            if i > 0 {
                self.output.push_str(", ");
            }
            self.write_value_id(*id);
            if self.config.show_types {
                self.output.push_str(": ");
                self.write_type(*value_type);
            }
        }
        self.output.push(')');

        if !function.returns.is_empty() {
            self.output.push_str(" -> (");
            for (i, (id, value_type)) in function
                .return_values_initial
                .iter()
                .zip(function.returns.iter())
                .enumerate()
            {
                if i > 0 {
                    self.output.push_str(", ");
                }
                self.write_value_id(*id);
                if self.config.show_types {
                    self.output.push_str(": ");
                    self.write_type(*value_type);
                }
            }
            self.output.push(')');
        }

        if function.call_count > 0 || function.size_estimate > 0 {
            let _ = write!(
                self.output,
                " /* calls: {}, size: {} */",
                function.call_count, function.size_estimate
            );
        }

        self.output.push_str(" {");
        self.write_newline();

        self.indent += 1;
        self.write_block(&function.body);

        if function.return_values != function.return_values_initial {
            self.write_indent();
            self.output.push_str("// final return values: ");
            for (i, id) in function.return_values.iter().enumerate() {
                if i > 0 {
                    self.output.push_str(", ");
                }
                self.write_value_id(*id);
            }
            self.write_newline();
        }
        self.indent -= 1;

        self.write_indent();
        self.output.push('}');
        self.write_newline();
    }

    fn write_block(&mut self, block: &Block) {
        for statement in &block.statements {
            self.write_statement(statement);
        }
    }

    fn write_region(&mut self, region: &Region) {
        for statement in &region.statements {
            self.write_statement(statement);
        }
        if !region.yields.is_empty() {
            self.write_indent();
            self.output.push_str("yield ");
            for (i, value) in region.yields.iter().enumerate() {
                if i > 0 {
                    self.output.push_str(", ");
                }
                self.write_value(value);
            }
            self.write_newline();
        }
    }

    fn write_statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Let { bindings, value } => {
                self.write_indent();
                self.output.push_str("let ");
                for (i, id) in bindings.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.write_binding_id(*id);
                }
                self.output.push_str(" := ");
                self.write_expression(value);
                self.write_newline();
            }

            Statement::MStore {
                offset,
                value,
                region,
            } => {
                self.write_indent();
                self.output.push_str("mstore(");
                self.write_value(offset);
                self.output.push_str(", ");
                self.write_value(value);
                self.output.push(')');
                if self.config.show_regions && *region != MemoryRegion::Unknown {
                    let _ = write!(self.output, " /* {} */", self.region_name(*region));
                }
                self.write_newline();
            }

            Statement::MStore8 {
                offset,
                value,
                region,
            } => {
                self.write_indent();
                self.output.push_str("mstore8(");
                self.write_value(offset);
                self.output.push_str(", ");
                self.write_value(value);
                self.output.push(')');
                if self.config.show_regions && *region != MemoryRegion::Unknown {
                    let _ = write!(self.output, " /* {} */", self.region_name(*region));
                }
                self.write_newline();
            }

            Statement::MCopy { dest, src, length } => {
                self.write_indent();
                self.output.push_str("mcopy(");
                self.write_value(dest);
                self.output.push_str(", ");
                self.write_value(src);
                self.output.push_str(", ");
                self.write_value(length);
                self.output.push(')');
                self.write_newline();
            }

            Statement::SStore {
                key,
                value,
                static_slot,
            } => {
                self.write_indent();
                self.output.push_str("sstore(");
                self.write_value(key);
                self.output.push_str(", ");
                self.write_value(value);
                self.output.push(')');
                if self.config.show_static_slots {
                    if let Some(slot) = static_slot {
                        let _ = write!(self.output, " /* slot: 0x{:x} */", slot);
                    }
                }
                self.write_newline();
            }

            Statement::TStore { key, value } => {
                self.write_indent();
                self.output.push_str("tstore(");
                self.write_value(key);
                self.output.push_str(", ");
                self.write_value(value);
                self.output.push(')');
                self.write_newline();
            }

            Statement::MappingSStore { key, slot, value } => {
                self.write_indent();
                self.output.push_str("mapping_sstore(");
                self.write_value(key);
                self.output.push_str(", ");
                self.write_value(slot);
                self.output.push_str(", ");
                self.write_value(value);
                self.output.push(')');
                self.write_newline();
            }

            Statement::If {
                condition,
                inputs,
                then_region,
                else_region,
                outputs,
            } => {
                self.write_indent();

                if !outputs.is_empty() {
                    self.output.push_str("let ");
                    for (i, id) in outputs.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.write_value_id(*id);
                    }
                    self.output.push_str(" := ");
                }

                self.output.push_str("if ");
                self.write_value(condition);

                if !inputs.is_empty() {
                    self.output.push_str(" [");
                    for (i, v) in inputs.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.write_value(v);
                    }
                    self.output.push(']');
                }

                self.output.push_str(" {");
                self.write_newline();

                self.indent += 1;
                self.write_region(then_region);
                self.indent -= 1;

                self.write_indent();
                self.output.push('}');

                if let Some(else_region) = else_region {
                    self.output.push_str(" else {");
                    self.write_newline();

                    self.indent += 1;
                    self.write_region(else_region);
                    self.indent -= 1;

                    self.write_indent();
                    self.output.push('}');
                }
                self.write_newline();
            }

            Statement::Switch {
                scrutinee,
                inputs,
                cases,
                default,
                outputs,
            } => {
                self.write_indent();

                if !outputs.is_empty() {
                    self.output.push_str("let ");
                    for (i, id) in outputs.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.write_value_id(*id);
                    }
                    self.output.push_str(" := ");
                }

                self.output.push_str("switch ");
                self.write_value(scrutinee);

                if !inputs.is_empty() {
                    self.output.push_str(" [");
                    for (i, v) in inputs.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.write_value(v);
                    }
                    self.output.push(']');
                }
                self.write_newline();

                for case in cases {
                    self.write_switch_case(case);
                }

                if let Some(default_region) = default {
                    self.write_indent();
                    self.output.push_str("default {");
                    self.write_newline();

                    self.indent += 1;
                    self.write_region(default_region);
                    self.indent -= 1;

                    self.write_indent();
                    self.output.push('}');
                    self.write_newline();
                }
            }

            Statement::For {
                initial_values,
                loop_variables,
                condition_statements,
                condition,
                body,
                post_input_variables,
                post,
                outputs,
            } => {
                self.write_indent();

                if !outputs.is_empty() {
                    self.output.push_str("let ");
                    for (i, id) in outputs.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.write_value_id(*id);
                    }
                    self.output.push_str(" := ");
                }

                self.output.push_str("for { ");

                for (i, (init, variable)) in
                    initial_values.iter().zip(loop_variables.iter()).enumerate()
                {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.write_value_id(*variable);
                    self.output.push_str(" := ");
                    self.write_value(init);
                }

                self.output.push_str(" }");
                self.write_newline();

                // Everything that makes up the loop is indented one level under
                // the `for` header so the condition, post and body blocks line
                // up and their braces match.
                self.indent += 1;

                if !condition_statements.is_empty() {
                    self.write_indent();
                    self.output.push_str("// condition statements:");
                    self.write_newline();
                    for statement in condition_statements {
                        self.write_statement(statement);
                    }
                }

                self.write_indent();
                self.output.push_str("condition: ");
                self.write_expression(condition);
                self.write_newline();

                self.write_indent();
                self.output.push_str("post");
                if !post_input_variables.is_empty() {
                    self.output.push_str(" (");
                    for (i, id) in post_input_variables.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.write_value_id(*id);
                    }
                    self.output.push(')');
                }
                self.output.push_str(" {");
                self.write_newline();
                self.indent += 1;
                self.write_region(post);
                self.indent -= 1;
                self.write_indent();
                self.output.push('}');
                self.write_newline();

                self.write_indent();
                self.output.push_str("body {");
                self.write_newline();
                self.indent += 1;
                self.write_region(body);
                self.indent -= 1;
                self.write_indent();
                self.output.push('}');
                self.write_newline();

                self.indent -= 1;
            }

            Statement::Break { .. } => {
                self.write_indent();
                self.output.push_str("break");
                self.write_newline();
            }

            Statement::Continue { .. } => {
                self.write_indent();
                self.output.push_str("continue");
                self.write_newline();
            }

            Statement::Leave { return_values } => {
                self.write_indent();
                self.output.push_str("leave");
                if !return_values.is_empty() {
                    self.output.push_str(" [");
                    for (i, v) in return_values.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.write_value(v);
                    }
                    self.output.push(']');
                }
                self.write_newline();
            }

            Statement::Revert { offset, length } => {
                self.write_indent();
                self.output.push_str("revert(");
                self.write_value(offset);
                self.output.push_str(", ");
                self.write_value(length);
                self.output.push(')');
                self.write_newline();
            }

            Statement::Return { offset, length } => {
                self.write_indent();
                self.output.push_str("return(");
                self.write_value(offset);
                self.output.push_str(", ");
                self.write_value(length);
                self.output.push(')');
                self.write_newline();
            }

            Statement::Stop => {
                self.write_indent();
                self.output.push_str("stop()");
                self.write_newline();
            }

            Statement::Invalid => {
                self.write_indent();
                self.output.push_str("invalid()");
                self.write_newline();
            }

            Statement::PanicRevert { code } => {
                self.write_indent();
                self.output.push_str(&format!("panic_revert(0x{code:02x})"));
                self.write_newline();
            }

            Statement::ErrorStringRevert { length, data } => {
                self.write_indent();
                self.output.push_str(&format!(
                    "error_string_revert({length}, {}_words)",
                    data.len()
                ));
                self.write_newline();
            }

            Statement::CustomErrorRevert {
                selector,
                arguments,
            } => {
                self.write_indent();
                let arg_strs: Vec<String> =
                    arguments.iter().map(|a| format!("v{}", a.id.0)).collect();
                self.output.push_str(&format!(
                    "custom_error_revert(0x{}, [{}])",
                    selector.to_str_radix(16),
                    arg_strs.join(", ")
                ));
                self.write_newline();
            }

            Statement::SelfDestruct { address } => {
                self.write_indent();
                self.output.push_str("selfdestruct(");
                self.write_value(address);
                self.output.push(')');
                self.write_newline();
            }

            Statement::ExternalCall {
                kind,
                gas,
                address,
                value,
                args_offset,
                args_length,
                ret_offset,
                ret_length,
                result,
            } => {
                self.write_indent();
                self.output.push_str("let ");
                self.write_value_id(*result);
                self.output.push_str(" := ");

                let call_name = match kind {
                    CallKind::Call => "call",
                    CallKind::CallCode => "callcode",
                    CallKind::DelegateCall => "delegatecall",
                    CallKind::StaticCall => "staticcall",
                };
                self.output.push_str(call_name);
                self.output.push('(');
                self.write_value(gas);
                self.output.push_str(", ");
                self.write_value(address);
                if let Some(value) = value {
                    self.output.push_str(", ");
                    self.write_value(value);
                }
                self.output.push_str(", ");
                self.write_value(args_offset);
                self.output.push_str(", ");
                self.write_value(args_length);
                self.output.push_str(", ");
                self.write_value(ret_offset);
                self.output.push_str(", ");
                self.write_value(ret_length);
                self.output.push(')');
                self.write_newline();
            }

            Statement::Create {
                kind,
                value,
                offset,
                length,
                salt,
                result,
            } => {
                self.write_indent();
                self.output.push_str("let ");
                self.write_value_id(*result);
                self.output.push_str(" := ");

                match kind {
                    CreateKind::Create => self.output.push_str("create("),
                    CreateKind::Create2 => self.output.push_str("create2("),
                }
                self.write_value(value);
                self.output.push_str(", ");
                self.write_value(offset);
                self.output.push_str(", ");
                self.write_value(length);
                if let Some(s) = salt {
                    self.output.push_str(", ");
                    self.write_value(s);
                }
                self.output.push(')');
                self.write_newline();
            }

            Statement::Log {
                offset,
                length,
                topics,
            } => {
                self.write_indent();
                let _ = write!(self.output, "log{}(", topics.len());
                self.write_value(offset);
                self.output.push_str(", ");
                self.write_value(length);
                for topic in topics {
                    self.output.push_str(", ");
                    self.write_value(topic);
                }
                self.output.push(')');
                self.write_newline();
            }

            Statement::CodeCopy {
                dest,
                offset,
                length,
            } => {
                self.write_indent();
                self.output.push_str("codecopy(");
                self.write_value(dest);
                self.output.push_str(", ");
                self.write_value(offset);
                self.output.push_str(", ");
                self.write_value(length);
                self.output.push(')');
                self.write_newline();
            }

            Statement::ExtCodeCopy {
                address,
                dest,
                offset,
                length,
            } => {
                self.write_indent();
                self.output.push_str("extcodecopy(");
                self.write_value(address);
                self.output.push_str(", ");
                self.write_value(dest);
                self.output.push_str(", ");
                self.write_value(offset);
                self.output.push_str(", ");
                self.write_value(length);
                self.output.push(')');
                self.write_newline();
            }

            Statement::ReturnDataCopy {
                dest,
                offset,
                length,
            } => {
                self.write_indent();
                self.output.push_str("returndatacopy(");
                self.write_value(dest);
                self.output.push_str(", ");
                self.write_value(offset);
                self.output.push_str(", ");
                self.write_value(length);
                self.output.push(')');
                self.write_newline();
            }

            Statement::DataCopy {
                dest,
                offset,
                length,
            } => {
                self.write_indent();
                self.output.push_str("datacopy(");
                self.write_value(dest);
                self.output.push_str(", ");
                self.write_value(offset);
                self.output.push_str(", ");
                self.write_value(length);
                self.output.push(')');
                self.write_newline();
            }

            Statement::CallDataCopy {
                dest,
                offset,
                length,
            } => {
                self.write_indent();
                self.output.push_str("calldatacopy(");
                self.write_value(dest);
                self.output.push_str(", ");
                self.write_value(offset);
                self.output.push_str(", ");
                self.write_value(length);
                self.output.push(')');
                self.write_newline();
            }

            Statement::Block(region) => {
                self.write_indent();
                self.output.push('{');
                self.write_newline();

                self.indent += 1;
                self.write_region(region);
                self.indent -= 1;

                self.write_indent();
                self.output.push('}');
                self.write_newline();
            }

            Statement::Expression(expression) => {
                self.write_indent();
                self.write_expression(expression);
                self.write_newline();
            }

            Statement::SetImmutable { key, value } => {
                self.write_indent();
                let _ = write!(self.output, "setimmutable(\"{}\", ", key);
                self.write_value(value);
                self.output.push(')');
                self.write_newline();
            }
        }
    }

    fn write_switch_case(&mut self, case: &SwitchCase) {
        self.write_indent();
        let _ = write!(self.output, "case 0x{:x} {{", case.value);
        self.write_newline();

        self.indent += 1;
        self.write_region(&case.body);
        self.indent -= 1;

        self.write_indent();
        self.output.push('}');
        self.write_newline();
    }

    fn write_expression(&mut self, expression: &Expression) {
        match expression {
            Expression::Literal { value, value_type } => {
                let _ = write!(self.output, "0x{:x}", value);
                if self.config.show_types && *value_type != Type::Int(BitWidth::I256) {
                    self.output.push_str(": ");
                    self.write_type(*value_type);
                }
            }

            Expression::Var(id) => {
                self.write_value_id(*id);
            }

            Expression::Binary {
                operation,
                lhs,
                rhs,
            } => {
                self.output.push_str(self.binop_name(*operation));
                self.output.push('(');
                self.write_value(lhs);
                self.output.push_str(", ");
                self.write_value(rhs);
                self.output.push(')');
            }

            Expression::Ternary { operation, a, b, n } => {
                self.output.push_str(self.binop_name(*operation));
                self.output.push('(');
                self.write_value(a);
                self.output.push_str(", ");
                self.write_value(b);
                self.output.push_str(", ");
                self.write_value(n);
                self.output.push(')');
            }

            Expression::Unary { operation, operand } => {
                let name = match operation {
                    UnaryOperation::IsZero => "iszero",
                    UnaryOperation::Not => "not",
                    UnaryOperation::Clz => "clz",
                };
                self.output.push_str(name);
                self.output.push('(');
                self.write_value(operand);
                self.output.push(')');
            }

            Expression::CallDataLoad { offset } => {
                self.output.push_str("calldataload(");
                self.write_value(offset);
                self.output.push(')');
            }

            Expression::CallValue => self.output.push_str("callvalue()"),
            Expression::Caller => self.output.push_str("caller()"),
            Expression::Origin => self.output.push_str("origin()"),
            Expression::CallDataSize => self.output.push_str("calldatasize()"),
            Expression::CodeSize => self.output.push_str("codesize()"),
            Expression::GasPrice => self.output.push_str("gasprice()"),

            Expression::ExtCodeSize { address } => {
                self.output.push_str("extcodesize(");
                self.write_value(address);
                self.output.push(')');
            }

            Expression::ReturnDataSize => self.output.push_str("returndatasize()"),

            Expression::ExtCodeHash { address } => {
                self.output.push_str("extcodehash(");
                self.write_value(address);
                self.output.push(')');
            }

            Expression::BlockHash { number } => {
                self.output.push_str("blockhash(");
                self.write_value(number);
                self.output.push(')');
            }

            Expression::Coinbase => self.output.push_str("coinbase()"),
            Expression::Timestamp => self.output.push_str("timestamp()"),
            Expression::Number => self.output.push_str("number()"),
            Expression::Difficulty => self.output.push_str("difficulty()"),
            Expression::GasLimit => self.output.push_str("gaslimit()"),
            Expression::ChainId => self.output.push_str("chainid()"),
            Expression::SelfBalance => self.output.push_str("selfbalance()"),
            Expression::BaseFee => self.output.push_str("basefee()"),

            Expression::BlobHash { index } => {
                self.output.push_str("blobhash(");
                self.write_value(index);
                self.output.push(')');
            }

            Expression::BlobBaseFee => self.output.push_str("blobbasefee()"),
            Expression::Gas => self.output.push_str("gas()"),
            Expression::MSize => self.output.push_str("msize()"),
            Expression::Address => self.output.push_str("address()"),

            Expression::Balance { address } => {
                self.output.push_str("balance(");
                self.write_value(address);
                self.output.push(')');
            }

            Expression::MLoad { offset, region } => {
                self.output.push_str("mload(");
                self.write_value(offset);
                self.output.push(')');
                if self.config.show_regions && *region != MemoryRegion::Unknown {
                    let _ = write!(self.output, " /* {} */", self.region_name(*region));
                }
            }

            Expression::SLoad { key, static_slot } => {
                self.output.push_str("sload(");
                self.write_value(key);
                self.output.push(')');
                if self.config.show_static_slots {
                    if let Some(slot) = static_slot {
                        let _ = write!(self.output, " /* slot: 0x{:x} */", slot);
                    }
                }
            }

            Expression::TLoad { key } => {
                self.output.push_str("tload(");
                self.write_value(key);
                self.output.push(')');
            }

            Expression::Call {
                function,
                arguments,
            } => {
                if let Some(name) = self.function_names.get(function) {
                    self.output.push_str(name);
                } else {
                    let _ = write!(self.output, "func_{}", function.0);
                }
                self.output.push('(');
                for (i, argument) in arguments.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.write_value(argument);
                }
                self.output.push(')');
            }

            Expression::Truncate { value, to } => {
                let _ = write!(self.output, "truncate<i{}>", to.bits());
                self.output.push('(');
                self.write_value(value);
                self.output.push(')');
            }

            Expression::ZeroExtend { value, to } => {
                let _ = write!(self.output, "zext<i{}>", to.bits());
                self.output.push('(');
                self.write_value(value);
                self.output.push(')');
            }

            Expression::SignExtendTo { value, to } => {
                let _ = write!(self.output, "sext<i{}>", to.bits());
                self.output.push('(');
                self.write_value(value);
                self.output.push(')');
            }

            Expression::Keccak256 { offset, length } => {
                self.output.push_str("keccak256(");
                self.write_value(offset);
                self.output.push_str(", ");
                self.write_value(length);
                self.output.push(')');
            }

            Expression::Keccak256Pair { word0, word1 } => {
                self.output.push_str("keccak256_pair(");
                self.write_value(word0);
                self.output.push_str(", ");
                self.write_value(word1);
                self.output.push(')');
            }

            Expression::Keccak256Single { word0 } => {
                self.output.push_str("keccak256_single(");
                self.write_value(word0);
                self.output.push(')');
            }

            Expression::MappingSLoad { key, slot } => {
                self.output.push_str("mapping_sload(");
                self.write_value(key);
                self.output.push_str(", ");
                self.write_value(slot);
                self.output.push(')');
            }

            Expression::DataOffset { id } => {
                let _ = write!(self.output, "dataoffset(\"{}\")", id);
            }

            Expression::DataSize { id } => {
                let _ = write!(self.output, "datasize(\"{}\")", id);
            }

            Expression::LoadImmutable { key } => {
                let _ = write!(self.output, "loadimmutable(\"{}\")", key);
            }

            Expression::LinkerSymbol { path } => {
                let _ = write!(self.output, "linkersymbol(\"{}\")", path);
            }
        }
    }

    fn write_value(&mut self, value: &Value) {
        self.write_value_id(value.id);
        if !self.config.show_types {
            return;
        }
        // Pointers and void are never narrowed by type inference, so keep the
        // statically assigned type. For integers, prefer the post-narrow
        // effective width when inference is attached; otherwise fall back to the
        // static type. The default i256 width is suppressed to reduce noise.
        let display_type = match (self.inferred_types, value.value_type) {
            (Some(inference), Type::Int(_)) => Type::Int(inference.effective_width(value.id)),
            _ => value.value_type,
        };
        if display_type != Type::Int(BitWidth::I256) {
            self.output.push_str(": ");
            self.write_type(display_type);
        }
    }

    /// Writes a binding value id, annotating it with the inferred effective
    /// width when type-inference results are attached. Bindings carry no static
    /// type of their own, so without inference no annotation is printed.
    fn write_binding_id(&mut self, id: ValueId) {
        self.write_value_id(id);
        if !self.config.show_types {
            return;
        }
        if let Some(inference) = self.inferred_types {
            let width = inference.effective_width(id);
            if width != BitWidth::I256 {
                self.output.push_str(": ");
                self.write_type(Type::Int(width));
            }
        }
    }

    fn write_value_id(&mut self, id: ValueId) {
        let _ = write!(self.output, "v{}", id.0);
    }

    fn write_type(&mut self, value_type: Type) {
        match value_type {
            Type::Int(w) => {
                let _ = write!(self.output, "i{}", w.bits());
            }
            Type::Ptr(space) => {
                let space_name = match space {
                    AddressSpace::Heap => "heap",
                    AddressSpace::Stack => "stack",
                    AddressSpace::Storage => "storage",
                    AddressSpace::Code => "code",
                };
                let _ = write!(self.output, "ptr<{}>", space_name);
            }
            Type::Void => {
                self.output.push_str("void");
            }
        }
    }

    fn binop_name(&self, operation: BinaryOperation) -> &'static str {
        match operation {
            BinaryOperation::Add => "add",
            BinaryOperation::Sub => "sub",
            BinaryOperation::Mul => "mul",
            BinaryOperation::Div => "div",
            BinaryOperation::SDiv => "sdiv",
            BinaryOperation::Mod => "mod",
            BinaryOperation::SMod => "smod",
            BinaryOperation::Exp => "exp",
            BinaryOperation::AddMod => "addmod",
            BinaryOperation::MulMod => "mulmod",
            BinaryOperation::And => "and",
            BinaryOperation::Or => "or",
            BinaryOperation::Xor => "xor",
            BinaryOperation::Shl => "shl",
            BinaryOperation::Shr => "shr",
            BinaryOperation::Sar => "sar",
            BinaryOperation::Lt => "lt",
            BinaryOperation::Gt => "gt",
            BinaryOperation::Slt => "slt",
            BinaryOperation::Sgt => "sgt",
            BinaryOperation::Eq => "eq",
            BinaryOperation::Byte => "byte",
            BinaryOperation::SignExtend => "signextend",
        }
    }

    fn region_name(&self, region: MemoryRegion) -> &'static str {
        match region {
            MemoryRegion::Scratch => "scratch",
            MemoryRegion::FreePointerSlot => "free_ptr",
            MemoryRegion::Dynamic => "dynamic",
            MemoryRegion::Unknown => "unknown",
        }
    }
}

impl Default for Printer<'_> {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to print an object to a string.
pub fn print_object(object: &Object) -> String {
    let mut printer = Printer::new();
    printer.print_object(object)
}

/// Convenience function to print an object annotated with inferred type widths.
///
/// Value ids are annotated with the post-narrow effective widths from
/// `type_info`; see [`Printer::set_type_info`].
pub fn print_object_with_types<'a>(object: &'a Object, type_info: &'a TypeInference) -> String {
    let mut printer = Printer::new();
    printer.set_type_info(type_info);
    printer.print_object(object)
}

/// Convenience function to print a function to a string.
pub fn print_function(function: &Function) -> String {
    let mut printer = Printer::new();
    printer.print_function(function)
}

/// Convenience function to print a statement to a string.
pub fn print_statement(statement: &Statement) -> String {
    let mut printer = Printer::new();
    printer.print_statement(statement)
}

/// Convenience function to print an expression to a string.
pub fn print_expression(expression: &Expression) -> String {
    Printer::new().print_expression(expression)
}

/// Implement Display for Object using the printer.
impl fmt::Display for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", print_object(self))
    }
}

/// Implement Display for Function using the printer.
impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", print_function(self))
    }
}

/// Implement Display for Statement using the printer.
impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", print_statement(self))
    }
}

/// Implement Display for Expression using the printer.
impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", print_expression(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num::BigUint;

    #[test]
    fn binding_shows_inferred_narrow_width() {
        use crate::type_inference::TypeInference;

        let mut object = Object::new("Test".to_string());
        object.code.statements.push(Statement::Let {
            bindings: vec![ValueId(0)],
            value: Expression::Literal {
                value: BigUint::from(0xffu64),
                value_type: Type::Int(BitWidth::I256),
            },
        });

        // Without type info the binding carries no annotation: bindings store no
        // type of their own.
        let plain = print_object(&object);
        assert!(plain.contains("let v0 := 0xff"), "got: {plain}");
        assert!(!plain.contains("v0: i"), "got: {plain}");

        // With inference attached, the i8 literal binding is annotated with its
        // post-narrow effective width.
        let mut inference = TypeInference::new();
        inference.infer_object(&object);
        assert_eq!(inference.effective_width(ValueId(0)), BitWidth::I8);
        let typed = print_object_with_types(&object, &inference);
        assert!(typed.contains("let v0: i8 := 0xff"), "got: {typed}");
    }

    #[test]
    fn subobject_bindings_use_subobject_inference() {
        use crate::type_inference::TypeInference;

        // Parent and subobject both bind ValueId(0), but to literals of
        // different widths. Because each object has its own ValueId namespace
        // and its own inference, the subobject's v0 must be annotated from the
        // subobject inference (i64), not the parent's (i8).
        let mut parent = Object::new("Parent".to_string());
        parent.code.statements.push(Statement::Let {
            bindings: vec![ValueId(0)],
            value: Expression::Literal {
                value: BigUint::from(0xffu64),
                value_type: Type::Int(BitWidth::I256),
            },
        });

        let mut subobject = Object::new("Parent_deployed".to_string());
        subobject.code.statements.push(Statement::Let {
            bindings: vec![ValueId(0)],
            value: Expression::Literal {
                value: BigUint::from(0xffffffffffffffffu64),
                value_type: Type::Int(BitWidth::I256),
            },
        });
        parent.subobjects.push(subobject);

        let mut inference = TypeInference::new();
        inference.infer_object_tree(&parent);

        let output = print_object_with_types(&parent, &inference);
        let subobject_section = output
            .split_once("object \"Parent_deployed\"")
            .expect("subobject section present")
            .1;

        // Parent v0 is the i8 literal; subobject v0 is the i64 literal. If the
        // subobject were printed against the parent inference it would wrongly
        // show i8 (or default to i1 for ids the parent never saw).
        assert!(
            output.contains("let v0: i8 := 0xff\n"),
            "parent binding should be i8:\n{output}"
        );
        assert!(
            subobject_section.contains("let v0: i64 := 0xffffffffffffffff"),
            "subobject binding should be i64 from its own inference:\n{subobject_section}"
        );
    }

    #[test]
    fn for_loop_prints_post_inputs_with_aligned_braces() {
        let for_statement = Statement::For {
            initial_values: vec![Value::int(ValueId(0))],
            loop_variables: vec![ValueId(1)],
            condition_statements: Vec::new(),
            condition: Expression::Var(ValueId(1)),
            body: Region::default(),
            post_input_variables: vec![ValueId(2)],
            post: Region::default(),
            outputs: Vec::new(),
        };

        let output = print_statement(&for_statement);

        // The post-region input variables must not be dropped.
        assert!(
            output.contains("post (v2) {"),
            "post inputs should be printed:\n{output}"
        );
        // Header at column 0; condition/post/body indented one level under it,
        // with matching open/close braces at that level.
        assert!(output.contains("for { v1 := v0 }\n"), "got:\n{output}");
        assert!(output.contains("\n    condition: v1\n"), "got:\n{output}");
        assert!(
            output.contains("\n    post (v2) {\n    }\n"),
            "got:\n{output}"
        );
        assert!(output.contains("\n    body {\n    }\n"), "got:\n{output}");
    }

    #[test]
    fn test_print_literal() {
        let expression = Expression::Literal {
            value: BigUint::from(42u64),
            value_type: Type::Int(BitWidth::I256),
        };
        let output = print_expression(&expression);
        assert_eq!(output, "0x2a");
    }

    #[test]
    fn test_print_binary_op() {
        let expression = Expression::Binary {
            operation: BinaryOperation::Add,
            lhs: Value::int(ValueId(0)),
            rhs: Value::int(ValueId(1)),
        };
        let output = print_expression(&expression);
        assert_eq!(output, "add(v0, v1)");
    }

    #[test]
    fn test_print_let_statement() {
        let statement = Statement::Let {
            bindings: vec![ValueId(2)],
            value: Expression::Binary {
                operation: BinaryOperation::Add,
                lhs: Value::int(ValueId(0)),
                rhs: Value::int(ValueId(1)),
            },
        };
        let output = print_statement(&statement);
        assert_eq!(output.trim(), "let v2 := add(v0, v1)");
    }

    #[test]
    fn test_print_mstore_with_region() {
        let statement = Statement::MStore {
            offset: Value::int(ValueId(0)),
            value: Value::int(ValueId(1)),
            region: MemoryRegion::Scratch,
        };
        let output = print_statement(&statement);
        assert!(output.contains("mstore(v0, v1)"));
        assert!(output.contains("/* scratch */"));
    }

    #[test]
    fn test_print_function() {
        let function = Function {
            id: FunctionId(0),
            name: "add_one".to_string(),
            parameters: vec![(ValueId(0), Type::Int(BitWidth::I256))],
            returns: vec![Type::Int(BitWidth::I256)],
            return_values_initial: vec![ValueId(1)],
            return_values: vec![ValueId(2)],
            body: Block {
                statements: vec![Statement::Let {
                    bindings: vec![ValueId(2)],
                    value: Expression::Binary {
                        operation: BinaryOperation::Add,
                        lhs: Value::int(ValueId(0)),
                        rhs: Value::new(ValueId(3), Type::Int(BitWidth::I256)),
                    },
                }],
            },
            call_count: 1,
            size_estimate: 5,
        };
        let output = print_function(&function);
        assert!(output.contains("function add_one"));
        assert!(output.contains("v0: i256"));
        assert!(output.contains("-> (v1: i256)"));
        assert!(output.contains("let v2 := add(v0, v3)"));
        assert!(output.contains("/* calls: 1, size: 5 */"));
    }

    #[test]
    fn test_subobject_function_names_are_scoped() {
        // Regression test for a printer bug where subobject Call expressions
        // resolved against the parent's function-name map: ids missing in the
        // parent printed as `func_<id>`, and id collisions printed the parent's
        // name (e.g. `extract_byte_array_length()` from a subobject).
        let parent_only = Function {
            id: FunctionId(0),
            name: "parent_only".to_string(),
            parameters: vec![],
            returns: vec![],
            return_values_initial: vec![],
            return_values: vec![],
            body: Block { statements: vec![] },
            call_count: 1,
            size_estimate: 1,
        };
        let sub_collides = Function {
            id: FunctionId(0),
            name: "sub_collides".to_string(),
            parameters: vec![],
            returns: vec![],
            return_values_initial: vec![],
            return_values: vec![],
            body: Block { statements: vec![] },
            call_count: 1,
            size_estimate: 1,
        };
        let sub_unique = Function {
            id: FunctionId(7),
            name: "sub_unique".to_string(),
            parameters: vec![],
            returns: vec![],
            return_values_initial: vec![],
            return_values: vec![],
            body: Block { statements: vec![] },
            call_count: 1,
            size_estimate: 1,
        };

        let call = |id| {
            Statement::Expression(Expression::Call {
                function: FunctionId(id),
                arguments: vec![],
            })
        };

        let mut parent_functions = BTreeMap::new();
        parent_functions.insert(parent_only.id, parent_only);
        let mut sub_functions = BTreeMap::new();
        sub_functions.insert(sub_collides.id, sub_collides);
        sub_functions.insert(sub_unique.id, sub_unique);

        let subobject = Object {
            name: "Sub".to_string(),
            code: Block {
                statements: vec![call(0), call(7)],
            },
            functions: sub_functions,
            subobjects: Vec::new(),
            data: BTreeMap::new(),
        };
        let object = Object {
            name: "Parent".to_string(),
            code: Block {
                statements: vec![call(0)],
            },
            functions: parent_functions,
            subobjects: vec![subobject],
            data: BTreeMap::new(),
        };

        let output = print_object(&object);

        let parent_header = output.find("object \"Parent\"").unwrap();
        let sub_header = output.find("object \"Sub\"").unwrap();
        let (parent_section, sub_section) = output.split_at(sub_header);
        let parent_section = &parent_section[parent_header..];

        assert!(
            parent_section.contains("parent_only()"),
            "parent should resolve its own FunctionId(0):\n{parent_section}",
        );
        assert!(
            sub_section.contains("sub_collides()"),
            "subobject's FunctionId(0) should print as sub_collides, not parent_only:\n{sub_section}",
        );
        assert!(
            sub_section.contains("sub_unique()"),
            "subobject's FunctionId(7) should resolve via the subobject map, not fall through to func_7:\n{sub_section}",
        );
        assert!(
            !sub_section.contains("parent_only"),
            "parent_only must not leak into subobject output:\n{sub_section}",
        );
        assert!(
            !sub_section.contains("func_7"),
            "missing-id fallback must not appear:\n{sub_section}",
        );
    }

    #[test]
    fn test_print_simple_object() {
        let object = Object {
            name: "Test".to_string(),
            code: Block {
                statements: vec![Statement::Let {
                    bindings: vec![ValueId(0)],
                    value: Expression::Literal {
                        value: BigUint::from(0u64),
                        value_type: Type::Int(BitWidth::I256),
                    },
                }],
            },
            functions: BTreeMap::new(),
            subobjects: Vec::new(),
            data: BTreeMap::new(),
        };
        let output = print_object(&object);
        assert!(output.contains("object \"Test\""));
        assert!(output.contains("code {"));
        assert!(output.contains("let v0 := 0x0"));
    }
}
