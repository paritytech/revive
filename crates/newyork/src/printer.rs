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
//!         let v0: i256 = 0x00
//!         let v1: i256 = calldataload(v0)
//!         mstore(v0, v1)
//!     }
//!
//!     function add_one(v2: i256) -> (v3: i256) {
//!         let v4: i256 = 0x01
//!         let v3 = add(v2, v4)
//!     }
//! }
//! ```

use crate::ir::{
    AddressSpace, BinaryOperation, BitWidth, Block, CallKind, CreateKind, Expression, Function,
    FunctionId, MemoryRegion, Object, Region, Statement, SwitchCase, Type, UnaryOperation, Value,
    ValueId,
};
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
}

impl<'a> Printer<'a> {
    /// Creates a new printer with default configuration.
    pub fn new() -> Self {
        Printer {
            config: PrinterConfig::default(),
            output: String::new(),
            indent: 0,
            function_names: BTreeMap::new(),
        }
    }

    /// Creates a new printer with the given configuration.
    pub fn with_config(config: PrinterConfig) -> Self {
        Printer {
            config,
            output: String::new(),
            indent: 0,
            function_names: BTreeMap::new(),
        }
    }

    /// Prints an IR object and returns the formatted string.
    pub fn print_object(&mut self, object: &'a Object) -> String {
        self.output.clear();
        self.indent = 0;
        self.function_names.clear();

        // Build function name lookup
        for (id, function) in &object.functions {
            self.function_names.insert(*id, &function.name);
        }

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

    // -------------------------------------------------------------------------
    // Internal writing methods
    // -------------------------------------------------------------------------

    fn write_indent(&mut self) {
        for _ in 0..self.indent * self.config.indent_size {
            self.output.push(' ');
        }
    }

    fn write_newline(&mut self) {
        self.output.push('\n');
    }

    fn write_object(&mut self, object: &'a Object) {
        self.write_indent();
        let _ = write!(self.output, "object \"{}\" {{", object.name);
        self.write_newline();

        self.indent += 1;

        // Print code block
        self.write_indent();
        self.output.push_str("code {");
        self.write_newline();

        self.indent += 1;
        self.write_block(&object.code);
        self.indent -= 1;

        self.write_indent();
        self.output.push('}');
        self.write_newline();

        // Print functions
        for function in object.functions.values() {
            self.write_newline();
            self.write_function(function);
        }

        // Print data sections
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

        // Print subobjects recursively
        for subobject in &object.subobjects {
            self.write_newline();
            self.write_object(subobject);
        }

        self.indent -= 1;
        self.write_indent();
        self.output.push('}');
        self.write_newline();
    }

    fn write_function(&mut self, function: &Function) {
        self.write_indent();
        let _ = write!(self.output, "function {}(", function.name);

        // Parameters
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

        // Return types
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

        // Print additional metadata
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

        // Print final return values if different from initial
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
                    self.write_value_id(*id);
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

                // Output bindings
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

                // Show inputs if any
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

                // Output bindings
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

                // Show inputs if any
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
                post,
                outputs,
                ..
            } => {
                self.write_indent();

                // Output bindings
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

                // Init values -> loop variables
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

                // Condition statements
                if !condition_statements.is_empty() {
                    self.indent += 1;
                    self.write_indent();
                    self.output.push_str("// condition_statements:");
                    self.write_newline();
                    for statement in condition_statements {
                        self.write_statement(statement);
                    }
                    self.indent -= 1;
                }

                self.write_indent();
                self.output.push_str("  ");
                self.write_expression(condition);
                self.write_newline();

                // Post region
                self.write_indent();
                self.output.push_str("  { // post");
                self.write_newline();
                self.indent += 1;
                self.write_region(post);
                self.indent -= 1;
                self.write_indent();
                self.output.push('}');
                self.write_newline();

                // Body region
                self.write_indent();
                self.output.push('{');
                self.write_newline();
                self.indent += 1;
                self.write_region(body);
                self.indent -= 1;
                self.write_indent();
                self.output.push('}');
                self.write_newline();
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
        if self.config.show_types && value.value_type != Type::Int(BitWidth::I256) {
            self.output.push_str(": ");
            self.write_type(value.value_type);
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
