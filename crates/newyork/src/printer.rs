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
    AddressSpace, BinOp, BitWidth, Block, CallKind, CreateKind, Expr, Function, FunctionId,
    MemoryRegion, Object, Region, Statement, SwitchCase, Type, UnaryOp, Value, ValueId,
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
    pub fn print_statement(&mut self, stmt: &Statement) -> String {
        self.output.clear();
        self.indent = 0;
        self.write_statement(stmt);
        std::mem::take(&mut self.output)
    }

    /// Prints an IR expression and returns the formatted string.
    pub fn print_expr(&mut self, expr: &Expr) -> String {
        self.output.clear();
        self.write_expr(expr);
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
        for (i, (id, ty)) in function.params.iter().enumerate() {
            if i > 0 {
                self.output.push_str(", ");
            }
            self.write_value_id(*id);
            if self.config.show_types {
                self.output.push_str(": ");
                self.write_type(*ty);
            }
        }
        self.output.push(')');

        // Return types
        if !function.returns.is_empty() {
            self.output.push_str(" -> (");
            for (i, (id, ty)) in function
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
                    self.write_type(*ty);
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
        for stmt in &block.statements {
            self.write_statement(stmt);
        }
    }

    fn write_region(&mut self, region: &Region) {
        for stmt in &region.statements {
            self.write_statement(stmt);
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

    fn write_statement(&mut self, stmt: &Statement) {
        match stmt {
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
                self.write_expr(value);
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
                init_values,
                loop_vars,
                condition_stmts,
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

                // Init values -> loop vars
                for (i, (init, var)) in init_values.iter().zip(loop_vars.iter()).enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.write_value_id(*var);
                    self.output.push_str(" := ");
                    self.write_value(init);
                }

                self.output.push_str(" }");
                self.write_newline();

                // Condition statements
                if !condition_stmts.is_empty() {
                    self.indent += 1;
                    self.write_indent();
                    self.output.push_str("// condition_stmts:");
                    self.write_newline();
                    for stmt in condition_stmts {
                        self.write_statement(stmt);
                    }
                    self.indent -= 1;
                }

                self.write_indent();
                self.output.push_str("  ");
                self.write_expr(condition);
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
                if let Some(val) = value {
                    self.output.push_str(", ");
                    self.write_value(val);
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

            Statement::Expr(expr) => {
                self.write_indent();
                self.write_expr(expr);
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

    fn write_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Literal { value, ty } => {
                let _ = write!(self.output, "0x{:x}", value);
                if self.config.show_types && *ty != Type::Int(BitWidth::I256) {
                    self.output.push_str(": ");
                    self.write_type(*ty);
                }
            }

            Expr::Var(id) => {
                self.write_value_id(*id);
            }

            Expr::Binary { op, lhs, rhs } => {
                self.output.push_str(self.binop_name(*op));
                self.output.push('(');
                self.write_value(lhs);
                self.output.push_str(", ");
                self.write_value(rhs);
                self.output.push(')');
            }

            Expr::Ternary { op, a, b, n } => {
                self.output.push_str(self.binop_name(*op));
                self.output.push('(');
                self.write_value(a);
                self.output.push_str(", ");
                self.write_value(b);
                self.output.push_str(", ");
                self.write_value(n);
                self.output.push(')');
            }

            Expr::Unary { op, operand } => {
                let name = match op {
                    UnaryOp::IsZero => "iszero",
                    UnaryOp::Not => "not",
                    UnaryOp::Clz => "clz",
                };
                self.output.push_str(name);
                self.output.push('(');
                self.write_value(operand);
                self.output.push(')');
            }

            Expr::CallDataLoad { offset } => {
                self.output.push_str("calldataload(");
                self.write_value(offset);
                self.output.push(')');
            }

            Expr::CallValue => self.output.push_str("callvalue()"),
            Expr::Caller => self.output.push_str("caller()"),
            Expr::Origin => self.output.push_str("origin()"),
            Expr::CallDataSize => self.output.push_str("calldatasize()"),
            Expr::CodeSize => self.output.push_str("codesize()"),
            Expr::GasPrice => self.output.push_str("gasprice()"),

            Expr::ExtCodeSize { address } => {
                self.output.push_str("extcodesize(");
                self.write_value(address);
                self.output.push(')');
            }

            Expr::ReturnDataSize => self.output.push_str("returndatasize()"),

            Expr::ExtCodeHash { address } => {
                self.output.push_str("extcodehash(");
                self.write_value(address);
                self.output.push(')');
            }

            Expr::BlockHash { number } => {
                self.output.push_str("blockhash(");
                self.write_value(number);
                self.output.push(')');
            }

            Expr::Coinbase => self.output.push_str("coinbase()"),
            Expr::Timestamp => self.output.push_str("timestamp()"),
            Expr::Number => self.output.push_str("number()"),
            Expr::Difficulty => self.output.push_str("difficulty()"),
            Expr::GasLimit => self.output.push_str("gaslimit()"),
            Expr::ChainId => self.output.push_str("chainid()"),
            Expr::SelfBalance => self.output.push_str("selfbalance()"),
            Expr::BaseFee => self.output.push_str("basefee()"),

            Expr::BlobHash { index } => {
                self.output.push_str("blobhash(");
                self.write_value(index);
                self.output.push(')');
            }

            Expr::BlobBaseFee => self.output.push_str("blobbasefee()"),
            Expr::Gas => self.output.push_str("gas()"),
            Expr::MSize => self.output.push_str("msize()"),
            Expr::Address => self.output.push_str("address()"),

            Expr::Balance { address } => {
                self.output.push_str("balance(");
                self.write_value(address);
                self.output.push(')');
            }

            Expr::MLoad { offset, region } => {
                self.output.push_str("mload(");
                self.write_value(offset);
                self.output.push(')');
                if self.config.show_regions && *region != MemoryRegion::Unknown {
                    let _ = write!(self.output, " /* {} */", self.region_name(*region));
                }
            }

            Expr::SLoad { key, static_slot } => {
                self.output.push_str("sload(");
                self.write_value(key);
                self.output.push(')');
                if self.config.show_static_slots {
                    if let Some(slot) = static_slot {
                        let _ = write!(self.output, " /* slot: 0x{:x} */", slot);
                    }
                }
            }

            Expr::TLoad { key } => {
                self.output.push_str("tload(");
                self.write_value(key);
                self.output.push(')');
            }

            Expr::Call { function, args } => {
                if let Some(name) = self.function_names.get(function) {
                    self.output.push_str(name);
                } else {
                    let _ = write!(self.output, "func_{}", function.0);
                }
                self.output.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.write_value(arg);
                }
                self.output.push(')');
            }

            Expr::Truncate { value, to } => {
                let _ = write!(self.output, "truncate<i{}>", to.bits());
                self.output.push('(');
                self.write_value(value);
                self.output.push(')');
            }

            Expr::ZeroExtend { value, to } => {
                let _ = write!(self.output, "zext<i{}>", to.bits());
                self.output.push('(');
                self.write_value(value);
                self.output.push(')');
            }

            Expr::SignExtendTo { value, to } => {
                let _ = write!(self.output, "sext<i{}>", to.bits());
                self.output.push('(');
                self.write_value(value);
                self.output.push(')');
            }

            Expr::Keccak256 { offset, length } => {
                self.output.push_str("keccak256(");
                self.write_value(offset);
                self.output.push_str(", ");
                self.write_value(length);
                self.output.push(')');
            }

            Expr::DataOffset { id } => {
                let _ = write!(self.output, "dataoffset(\"{}\")", id);
            }

            Expr::DataSize { id } => {
                let _ = write!(self.output, "datasize(\"{}\")", id);
            }

            Expr::LoadImmutable { key } => {
                let _ = write!(self.output, "loadimmutable(\"{}\")", key);
            }

            Expr::LinkerSymbol { path } => {
                let _ = write!(self.output, "linkersymbol(\"{}\")", path);
            }
        }
    }

    fn write_value(&mut self, value: &Value) {
        self.write_value_id(value.id);
        if self.config.show_types && value.ty != Type::Int(BitWidth::I256) {
            self.output.push_str(": ");
            self.write_type(value.ty);
        }
    }

    fn write_value_id(&mut self, id: ValueId) {
        let _ = write!(self.output, "v{}", id.0);
    }

    fn write_type(&mut self, ty: Type) {
        match ty {
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

    fn binop_name(&self, op: BinOp) -> &'static str {
        match op {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::Div => "div",
            BinOp::SDiv => "sdiv",
            BinOp::Mod => "mod",
            BinOp::SMod => "smod",
            BinOp::Exp => "exp",
            BinOp::AddMod => "addmod",
            BinOp::MulMod => "mulmod",
            BinOp::And => "and",
            BinOp::Or => "or",
            BinOp::Xor => "xor",
            BinOp::Shl => "shl",
            BinOp::Shr => "shr",
            BinOp::Sar => "sar",
            BinOp::Lt => "lt",
            BinOp::Gt => "gt",
            BinOp::Slt => "slt",
            BinOp::Sgt => "sgt",
            BinOp::Eq => "eq",
            BinOp::Byte => "byte",
            BinOp::SignExtend => "signextend",
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
pub fn print_statement(stmt: &Statement) -> String {
    let mut printer = Printer::new();
    printer.print_statement(stmt)
}

/// Convenience function to print an expression to a string.
pub fn print_expr(expr: &Expr) -> String {
    Printer::new().print_expr(expr)
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

/// Implement Display for Expr using the printer.
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", print_expr(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num::BigUint;

    #[test]
    fn test_print_literal() {
        let expr = Expr::Literal {
            value: BigUint::from(42u64),
            ty: Type::Int(BitWidth::I256),
        };
        let output = print_expr(&expr);
        assert_eq!(output, "0x2a");
    }

    #[test]
    fn test_print_binary_op() {
        let expr = Expr::Binary {
            op: BinOp::Add,
            lhs: Value::int(ValueId(0)),
            rhs: Value::int(ValueId(1)),
        };
        let output = print_expr(&expr);
        assert_eq!(output, "add(v0, v1)");
    }

    #[test]
    fn test_print_let_statement() {
        let stmt = Statement::Let {
            bindings: vec![ValueId(2)],
            value: Expr::Binary {
                op: BinOp::Add,
                lhs: Value::int(ValueId(0)),
                rhs: Value::int(ValueId(1)),
            },
        };
        let output = print_statement(&stmt);
        assert_eq!(output.trim(), "let v2 := add(v0, v1)");
    }

    #[test]
    fn test_print_mstore_with_region() {
        let stmt = Statement::MStore {
            offset: Value::int(ValueId(0)),
            value: Value::int(ValueId(1)),
            region: MemoryRegion::Scratch,
        };
        let output = print_statement(&stmt);
        assert!(output.contains("mstore(v0, v1)"));
        assert!(output.contains("/* scratch */"));
    }

    #[test]
    fn test_print_function() {
        let function = Function {
            id: FunctionId(0),
            name: "add_one".to_string(),
            params: vec![(ValueId(0), Type::Int(BitWidth::I256))],
            returns: vec![Type::Int(BitWidth::I256)],
            return_values_initial: vec![ValueId(1)],
            return_values: vec![ValueId(2)],
            body: Block {
                statements: vec![Statement::Let {
                    bindings: vec![ValueId(2)],
                    value: Expr::Binary {
                        op: BinOp::Add,
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
                    value: Expr::Literal {
                        value: BigUint::from(0u64),
                        ty: Type::Int(BitWidth::I256),
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
