//! The YUL AST visitor interface definitions.

use crate::{
    lexer::token::location::Location,
    parser::{
        identifier::Identifier,
        statement::{
            assignment::Assignment,
            block::Block,
            code::Code,
            expression::{function_call::FunctionCall, literal::Literal, Expression},
            for_loop::ForLoop,
            function_definition::FunctionDefinition,
            if_conditional::IfConditional,
            object::Object,
            switch::{case::Case, Switch},
            variable_declaration::VariableDeclaration,
            Statement,
        },
    },
};

/// This trait is implemented by all AST node types.
///
/// It allows to define how the AST is visited on a per-node basis.
pub trait AstNode: std::fmt::Debug {
    /// Accept the given [AstVisitor].
    ///
    /// This is supposed to call the corresponding `AstVisitor::visit_*` method.
    fn accept(&self, ast_visitor: &mut impl AstVisitor);

    /// Let any child nodes accept the given [AstVisitor].
    ///
    /// This is supposed visit child nodes in the correct order.
    ///
    /// Visitor implementations are supposed to call this method for traversing.
    fn visit_children(&self, _ast_visitor: &mut impl AstVisitor) {}

    /// Returns the lexer (source) location of the node.
    fn location(&self) -> Location;
}

/// This trait allows implementing custom AST visitor logic for each node type,
/// without needing to worry about how the nodes must be traversed.
///
/// The only thing the visitor needs to ensure is to call the nodes
/// [AstNode::visit_children] method (in any trait method).
/// This simplifies the implementation fo AST visitors a lot (see below example).
///
/// Default implementations which do nothing except for visiting children
/// are provided for each node type.
///
/// The [AstVisitor::visit] method is the generic visitor method,
/// which is seen by all nodes.
///
/// Visited nodes are given read only access (non-mutable refernces) on purpose,
/// as it's a compiler design best practice to not mutate the AST after parsing.
/// Instead, mutable access to the [AstVisitor] instance itself is,
/// provided, allowing to build a new representation instead if needed.
///
/// # Example
///
/// ```rust
/// use revive_yul::visitor::*;
///
/// /// A very simple visitor that counts all nodes in the AST.
/// #[derive(Default, Debug)]
/// pub struct CountVisitor(usize);
///
/// impl AstVisitor for CountVisitor {
///     /// Increment the counter for ech node we visit.
///     fn visit(&mut self, node: &impl AstNode) {
///         node.visit_children(self);
///         self.0 += 1;
///     }
///
///     /*
///
///     /// If we were interested in a per-statement breakdown of the AST,
///     /// we would implement all `visit_*` methods to cover each node here.
///     fn visit_assignment(&mut self, node: &Assignment) {
///         self.visit_children(node);
///         self.assignment_count += 1;
///     }
///
///     fn visit_block(&mut self, node: &Block) {
///         self.visit_children(node);
///         self.block_count += 1;
///     }
///
///     */
/// }
///
/// ```
pub trait AstVisitor {
    /// The generic visitor logic for all node types is executed upon visiting any statement.
    fn visit(&mut self, node: &impl AstNode);

    /// The logic to execute upon visiting [Assignment] statements.
    fn visit_assignment(&mut self, node: &Assignment) {
        self.visit(node);
    }

    /// The logic to execute upon visiting any [Block].
    fn visit_block(&mut self, node: &Block) {
        self.visit(node);
    }

    /// The logic to execute upon visiting [Case] statements.
    fn visit_case(&mut self, node: &Case) {
        self.visit(node);
    }

    /// The logic to execute upon visiting [Code] statements.
    fn visit_code(&mut self, node: &Code) {
        self.visit(node);
    }

    /// The logic to execute upon visiting any [Expression].
    fn visit_expression(&mut self, node: &Expression) {
        self.visit(node);
    }

    /// The logic to execute upon visiting [ForLoop] statements.
    fn visit_for_loop(&mut self, node: &ForLoop) {
        self.visit(node);
    }

    /// The logic to execute upon visiting [FunctionCall] statements.
    fn visit_function_call(&mut self, node: &FunctionCall) {
        self.visit(node);
    }

    /// The logic to execute upon visiting any [FunctionDefinition].
    fn visit_function_definition(&mut self, node: &FunctionDefinition) {
        self.visit(node);
    }

    /// The logic to execute upon visiting any [Identifier].
    fn visit_identifier(&mut self, node: &Identifier) {
        self.visit(node);
    }

    /// The logic to execute upon visiting [IfConditional] statements.
    fn visit_if_conditional(&mut self, node: &IfConditional) {
        self.visit(node);
    }

    /// The logic to execute upon visiting any [Literal].
    fn visit_literal(&mut self, node: &Literal) {
        self.visit(node);
    }

    /// The logic to execute upon visiting [Object] definitions.
    fn visit_object(&mut self, node: &Object) {
        self.visit(node);
    }

    /// The logic to execute upon visiting any YUL [Statement].
    fn visit_statement(&mut self, node: &Statement) {
        self.visit(node);
    }

    /// The logic to execute upon visiting [Switch] statements.
    fn visit_switch(&mut self, node: &Switch) {
        self.visit(node);
    }

    /// The logic to execute upon visiting any [VariableDeclaration].
    fn visit_variable_declaration(&mut self, node: &VariableDeclaration) {
        self.visit(node);
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        lexer::Lexer,
        parser::{
            identifier::Identifier,
            statement::{
                assignment::Assignment,
                block::Block,
                code::Code,
                expression::{function_call::FunctionCall, literal::Literal, Expression},
                for_loop::ForLoop,
                function_definition::FunctionDefinition,
                if_conditional::IfConditional,
                object::Object,
                switch::{case::Case, Switch},
                variable_declaration::VariableDeclaration,
                Statement,
            },
        },
    };

    use super::{AstNode, AstVisitor};

    /// The [Printer] visitor builds the AST back into its textual representation.
    #[derive(Default)]
    struct Printer {
        /// The print buffer.
        buffer: String,
        /// The current indentation level.
        indentation: usize,
    }

    impl Printer {
        /// Append a newline with the current identation to the print buffer.
        fn newline(&mut self) {
            self.buffer.push_str("\n");
            self.indent();
        }

        /// Append the current identation to the print buffer.
        fn indent(&mut self) {
            for _ in 0..self.indentation {
                self.buffer.push_str("  ");
            }
        }

        /// Append the given `nodes` comma-separated.
        fn separate(&mut self, nodes: &[impl AstNode]) {
            for (index, argument) in nodes.iter().enumerate() {
                argument.accept(self);

                if index < nodes.len() - 1 {
                    self.buffer.push_str(", ");
                }
            }
        }
    }

    impl AstVisitor for Printer {
        fn visit(&mut self, node: &impl AstNode) {
            node.accept(self);
        }

        fn visit_assignment(&mut self, node: &Assignment) {
            self.separate(&node.bindings);

            self.buffer.push_str(" := ");

            node.initializer.visit_children(self);
        }

        fn visit_block(&mut self, node: &Block) {
            self.newline();
            self.buffer.push_str("{");
            self.indentation += 1;

            node.visit_children(self);

            self.indentation -= 1;
            self.newline();
            self.buffer.push_str("}");
        }

        fn visit_case(&mut self, node: &Case) {
            self.newline();
            self.buffer.push_str("case ");
            node.visit_children(self);
        }

        fn visit_code(&mut self, node: &Code) {
            self.buffer.push_str("code ");
            node.visit_children(self);
        }

        fn visit_expression(&mut self, node: &Expression) {
            node.visit_children(self);
        }

        fn visit_for_loop(&mut self, node: &ForLoop) {
            self.buffer.push_str("for ");
            node.visit_children(self);
        }

        fn visit_function_call(&mut self, node: &FunctionCall) {
            self.buffer.push_str(&format!("{}", node.name));
            self.buffer.push_str("(");

            self.separate(&node.arguments);

            self.buffer.push_str(")");
        }

        fn visit_function_definition(&mut self, node: &FunctionDefinition) {
            self.buffer
                .push_str(&format!("function {}", node.identifier));

            self.buffer.push_str("(");
            self.separate(&node.arguments);
            self.buffer.push_str(")");

            self.buffer.push_str(" -> ");
            self.separate(&node.result);

            node.body.accept(self);
        }

        fn visit_identifier(&mut self, node: &Identifier) {
            self.buffer.push_str(&node.inner);
        }

        fn visit_if_conditional(&mut self, node: &IfConditional) {
            self.buffer.push_str("if ");
            node.visit_children(self);
        }

        fn visit_literal(&mut self, node: &Literal) {
            self.buffer.push_str(&format!("{}", node.inner));
        }

        fn visit_object(&mut self, node: &Object) {
            self.newline();
            self.buffer.push_str("object \"");
            self.buffer.push_str(&node.identifier);
            self.buffer.push_str("\" {");
            self.indentation += 1;
            self.newline();

            node.visit_children(self);

            self.indentation -= 1;
            self.newline();
            self.buffer.push_str("}");
        }

        fn visit_statement(&mut self, node: &Statement) {
            self.newline();
            node.visit_children(self);
        }

        fn visit_switch(&mut self, node: &Switch) {
            self.buffer.push_str("switch ");
            node.visit_children(self);
        }

        fn visit_variable_declaration(&mut self, node: &VariableDeclaration) {
            self.buffer.push_str("let ");
            self.separate(&node.bindings);

            if let Some(initializer) = node.expression.as_ref() {
                self.buffer.push_str(" := ");
                initializer.visit_children(self);
            }
        }
    }

    const ERC20: &str = r#"/// @use-src 0:"crates/integration/contracts/ERC20.sol"
object "ERC20_247" {
    code {
        {
            /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
            mstore(64, memoryguard(0x80))
            if callvalue() { revert(0, 0) }
            let oldLen := extract_byte_array_length(sload(/** @src 0:1542:1563  "\"Solidity by Example\"" */ 0x03))
            /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
            if gt(oldLen, 31)
            {
                mstore(/** @src -1:-1:-1 */ 0, /** @src 0:1542:1563  "\"Solidity by Example\"" */ 0x03)
                /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                let data := keccak256(/** @src -1:-1:-1 */ 0, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ 0x20)
                let deleteStart := add(data, 1)
                deleteStart := data
                let _1 := add(data, shr(5, add(oldLen, 31)))
                let start := data
                for { } lt(start, _1) { start := add(start, 1) }
                {
                    sstore(start, /** @src -1:-1:-1 */ 0)
                }
            }
            /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
            sstore(/** @src 0:1542:1563  "\"Solidity by Example\"" */ 0x03, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ add("Solidity by Example", 38))
            let oldLen_1 := extract_byte_array_length(sload(/** @src 0:1592:1601  "\"SOLBYEX\"" */ 0x04))
            /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
            if gt(oldLen_1, 31)
            {
                mstore(/** @src -1:-1:-1 */ 0, /** @src 0:1592:1601  "\"SOLBYEX\"" */ 0x04)
                /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                let data_1 := keccak256(/** @src -1:-1:-1 */ 0, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ 0x20)
                let deleteStart_1 := add(data_1, 1)
                deleteStart_1 := data_1
                let _2 := add(data_1, shr(5, add(oldLen_1, 31)))
                let start_1 := data_1
                for { } lt(start_1, _2) { start_1 := add(start_1, 1) }
                {
                    sstore(start_1, /** @src -1:-1:-1 */ 0)
                }
            }
            /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
            sstore(/** @src 0:1592:1601  "\"SOLBYEX\"" */ 0x04, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ add("SOLBYEX", 14))
            sstore(/** @src 0:1631:1633  "18" */ 0x05, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ or(and(sload(/** @src 0:1631:1633  "18" */ 0x05), /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ not(255)), /** @src 0:1631:1633  "18" */ 0x12))
            /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
            let _3 := mload(64)
            let _4 := datasize("ERC20_247_deployed")
            codecopy(_3, dataoffset("ERC20_247_deployed"), _4)
            return(_3, _4)
        }
        function extract_byte_array_length(data) -> length
        {
            length := shr(1, data)
            let outOfPlaceEncoding := and(data, 1)
            if iszero(outOfPlaceEncoding) { length := and(length, 0x7f) }
            if eq(outOfPlaceEncoding, lt(length, 32))
            {
                mstore(0, shl(224, 0x4e487b71))
                mstore(4, 0x22)
                revert(0, 0x24)
            }
        }
    }
    /// @use-src 0:"crates/integration/contracts/ERC20.sol"
    object "ERC20_247_deployed" {
        code {
            {
                /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                mstore(64, memoryguard(0x80))
                if iszero(lt(calldatasize(), 4))
                {
                    switch shr(224, calldataload(0))
                    case 0x06fdde03 {
                        if callvalue() { revert(0, 0) }
                        if slt(add(calldatasize(), not(3)), 0) { revert(0, 0) }
                        /// @src 0:1521:1563  "string public name = \"Solidity by Example\""
                        let value := /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ 0
                        let offset := 0
                        offset := 0
                        let memPtr := mload(64)
                        let ret := 0
                        let slotValue := sload(/** @src 0:1521:1563  "string public name = \"Solidity by Example\"" */ 3)
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        let length := 0
                        length := shr(1, slotValue)
                        let outOfPlaceEncoding := and(slotValue, 1)
                        if iszero(outOfPlaceEncoding) { length := and(length, 0x7f) }
                        if eq(outOfPlaceEncoding, lt(length, 32))
                        {
                            mstore(0, shl(224, 0x4e487b71))
                            mstore(4, 0x22)
                            revert(0, 0x24)
                        }
                        mstore(memPtr, length)
                        switch outOfPlaceEncoding
                        case 0 {
                            mstore(add(memPtr, 32), and(slotValue, not(255)))
                            ret := add(add(memPtr, shl(5, iszero(iszero(length)))), 32)
                        }
                        case 1 {
                            mstore(0, /** @src 0:1521:1563  "string public name = \"Solidity by Example\"" */ 3)
                            /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                            let dataPos := keccak256(0, 32)
                            let i := 0
                            for { } lt(i, length) { i := add(i, 32) }
                            {
                                mstore(add(add(memPtr, i), 32), sload(dataPos))
                                dataPos := add(dataPos, 1)
                            }
                            ret := add(add(memPtr, i), 32)
                        }
                        let newFreePtr := add(memPtr, and(add(sub(ret, memPtr), 31), not(31)))
                        if or(gt(newFreePtr, 0xffffffffffffffff), lt(newFreePtr, memPtr))
                        {
                            mstore(0, shl(224, 0x4e487b71))
                            mstore(4, 0x41)
                            revert(0, 0x24)
                        }
                        mstore(64, newFreePtr)
                        value := memPtr
                        let memPos := mload(64)
                        return(memPos, sub(abi_encode_string(memPos, memPtr), memPos))
                    }
                    case 0x095ea7b3 {
                        if callvalue() { revert(0, 0) }
                        if slt(add(calldatasize(), not(3)), 64) { revert(0, 0) }
                        let value0 := abi_decode_address_3473()
                        let value_1 := calldataload(36)
                        mstore(0, /** @src 0:1974:1984  "msg.sender" */ caller())
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        mstore(32, /** @src 0:1964:1973  "allowance" */ 0x02)
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        let dataSlot := keccak256(0, 64)
                        /// @src 0:1964:1994  "allowance[msg.sender][spender]"
                        let dataSlot_1 := /** @src -1:-1:-1 */ 0
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        mstore(/** @src -1:-1:-1 */ 0, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ and(/** @src 0:1964:1994  "allowance[msg.sender][spender]" */ value0, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ sub(shl(160, 1), 1)))
                        mstore(0x20, /** @src 0:1964:1985  "allowance[msg.sender]" */ dataSlot)
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        dataSlot_1 := keccak256(/** @src -1:-1:-1 */ 0, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ 0x40)
                        sstore(/** @src 0:1964:1994  "allowance[msg.sender][spender]" */ dataSlot_1, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ value_1)
                        /// @src 0:2018:2055  "Approval(msg.sender, spender, amount)"
                        let _1 := /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ mload(64)
                        mstore(_1, value_1)
                        /// @src 0:2018:2055  "Approval(msg.sender, spender, amount)"
                        log3(_1, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ 32, /** @src 0:2018:2055  "Approval(msg.sender, spender, amount)" */ 0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925, /** @src 0:1974:1984  "msg.sender" */ caller(), /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ and(/** @src 0:2018:2055  "Approval(msg.sender, spender, amount)" */ value0, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ sub(shl(160, 1), 1)))
                        let memPos_1 := mload(64)
                        mstore(memPos_1, 1)
                        return(memPos_1, 32)
                    }
                    case 0x18160ddd {
                        if callvalue() { revert(0, 0) }
                        if slt(add(calldatasize(), not(3)), 0) { revert(0, 0) }
                        let _2 := sload(0)
                        let memPos_2 := mload(64)
                        mstore(memPos_2, _2)
                        return(memPos_2, 32)
                    }
                    case 0x23b872dd {
                        if callvalue() { revert(0, 0) }
                        if slt(add(calldatasize(), not(3)), 96) { revert(0, 0) }
                        let value0_1 := abi_decode_address_3473()
                        let value1 := abi_decode_address()
                        let value_2 := calldataload(68)
                        let _3 := and(value0_1, sub(shl(160, 1), 1))
                        mstore(0, _3)
                        mstore(32, /** @src 0:2223:2232  "allowance" */ 0x02)
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        let dataSlot_2 := keccak256(0, 64)
                        /// @src 0:2223:2252  "allowance[sender][msg.sender]"
                        let dataSlot_3 := /** @src -1:-1:-1 */ 0
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        mstore(/** @src -1:-1:-1 */ 0, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ and(/** @src 0:2241:2251  "msg.sender" */ caller(), /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ sub(shl(160, 1), 1)))
                        mstore(0x20, /** @src 0:2223:2252  "allowance[sender][msg.sender]" */ dataSlot_2)
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        dataSlot_3 := keccak256(/** @src -1:-1:-1 */ 0, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ 0x40)
                        sstore(/** @src 0:2223:2252  "allowance[sender][msg.sender]" */ dataSlot_3, /** @src 0:2223:2262  "allowance[sender][msg.sender] -= amount" */ checked_sub_uint256(/** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ sload(/** @src 0:2223:2252  "allowance[sender][msg.sender]" */ dataSlot_3), /** @src 0:2223:2262  "allowance[sender][msg.sender] -= amount" */ value_2))
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        mstore(0, _3)
                        mstore(32, 1)
                        let dataSlot_4 := keccak256(0, 64)
                        sstore(dataSlot_4, /** @src 0:2272:2299  "balanceOf[sender] -= amount" */ checked_sub_uint256(/** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ sload(/** @src 0:2272:2299  "balanceOf[sender] -= amount" */ dataSlot_4), value_2))
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        let _4 := and(value1, sub(shl(160, 1), 1))
                        mstore(0, _4)
                        mstore(32, 1)
                        let dataSlot_5 := keccak256(0, 64)
                        sstore(dataSlot_5, /** @src 0:2309:2339  "balanceOf[recipient] += amount" */ checked_add_uint256(/** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ sload(/** @src 0:2309:2339  "balanceOf[recipient] += amount" */ dataSlot_5), value_2))
                        /// @src 0:2354:2389  "Transfer(sender, recipient, amount)"
                        let _5 := /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ mload(64)
                        mstore(_5, value_2)
                        /// @src 0:2354:2389  "Transfer(sender, recipient, amount)"
                        log3(_5, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ 32, /** @src 0:2354:2389  "Transfer(sender, recipient, amount)" */ 0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef, _3, _4)
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        let memPos_3 := mload(64)
                        mstore(memPos_3, 1)
                        return(memPos_3, 32)
                    }
                    case 0x313ce567 {
                        if callvalue() { revert(0, 0) }
                        if slt(add(calldatasize(), not(3)), 0) { revert(0, 0) }
                        let value_3 := and(sload(/** @src 0:1607:1633  "uint8 public decimals = 18" */ 5), /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ 0xff)
                        let memPos_4 := mload(64)
                        mstore(memPos_4, value_3)
                        return(memPos_4, 32)
                    }
                    case 0x42966c68 {
                        if callvalue() { revert(0, 0) }
                        if slt(add(calldatasize(), not(3)), 32) { revert(0, 0) }
                        let value_4 := calldataload(4)
                        mstore(0, /** @src 0:2655:2665  "msg.sender" */ caller())
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        mstore(32, 1)
                        let dataSlot_6 := keccak256(0, 64)
                        sstore(dataSlot_6, /** @src 0:2645:2676  "balanceOf[msg.sender] -= amount" */ checked_sub_uint256(/** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ sload(/** @src 0:2645:2676  "balanceOf[msg.sender] -= amount" */ dataSlot_6), value_4))
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        sstore(0, /** @src 0:2686:2707  "totalSupply -= amount" */ checked_sub_uint256(/** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ sload(0), /** @src 0:2686:2707  "totalSupply -= amount" */ value_4))
                        /// @src 0:2722:2762  "Transfer(msg.sender, address(0), amount)"
                        let _6 := /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ mload(64)
                        mstore(_6, value_4)
                        /// @src 0:2722:2762  "Transfer(msg.sender, address(0), amount)"
                        log3(_6, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ 32, /** @src 0:2722:2762  "Transfer(msg.sender, address(0), amount)" */ 0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef, /** @src 0:2655:2665  "msg.sender" */ caller(), /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ 0)
                        return(0, 0)
                    }
                    case 0x70a08231 {
                        if callvalue() { revert(0, 0) }
                        if slt(add(calldatasize(), not(3)), 32) { revert(0, 0) }
                        mstore(0, and(abi_decode_address_3473(), sub(shl(160, 1), 1)))
                        mstore(32, /** @src 0:1407:1448  "mapping(address => uint) public balanceOf" */ 1)
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        let _7 := sload(keccak256(0, 64))
                        let memPos_5 := mload(64)
                        mstore(memPos_5, _7)
                        return(memPos_5, 32)
                    }
                    case 0x95d89b41 {
                        if callvalue() { revert(0, 0) }
                        if slt(add(calldatasize(), not(3)), 0) { revert(0, 0) }
                        /// @src 0:1569:1601  "string public symbol = \"SOLBYEX\""
                        let value_5 := /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ 0
                        let offset_1 := 0
                        offset_1 := 0
                        let memPtr_1 := mload(64)
                        let ret_1 := 0
                        let slotValue_1 := sload(4)
                        let length_1 := 0
                        length_1 := shr(1, slotValue_1)
                        let outOfPlaceEncoding_1 := and(slotValue_1, 1)
                        if iszero(outOfPlaceEncoding_1)
                        {
                            length_1 := and(length_1, 0x7f)
                        }
                        if eq(outOfPlaceEncoding_1, lt(length_1, 32))
                        {
                            mstore(0, shl(224, 0x4e487b71))
                            mstore(4, 0x22)
                            revert(0, 0x24)
                        }
                        mstore(memPtr_1, length_1)
                        switch outOfPlaceEncoding_1
                        case 0 {
                            mstore(add(memPtr_1, 32), and(slotValue_1, not(255)))
                            ret_1 := add(add(memPtr_1, shl(5, iszero(iszero(length_1)))), 32)
                        }
                        case 1 {
                            mstore(0, 4)
                            let dataPos_1 := keccak256(0, 32)
                            let i_1 := 0
                            for { } lt(i_1, length_1) { i_1 := add(i_1, 32) }
                            {
                                mstore(add(add(memPtr_1, i_1), 32), sload(dataPos_1))
                                dataPos_1 := add(dataPos_1, 1)
                            }
                            ret_1 := add(add(memPtr_1, i_1), 32)
                        }
                        let newFreePtr_1 := add(memPtr_1, and(add(sub(ret_1, memPtr_1), 31), not(31)))
                        if or(gt(newFreePtr_1, 0xffffffffffffffff), lt(newFreePtr_1, memPtr_1))
                        {
                            mstore(0, shl(224, 0x4e487b71))
                            mstore(4, 0x41)
                            revert(0, 0x24)
                        }
                        mstore(64, newFreePtr_1)
                        value_5 := memPtr_1
                        let memPos_6 := mload(64)
                        return(memPos_6, sub(abi_encode_string(memPos_6, memPtr_1), memPos_6))
                    }
                    case 0xa0712d68 {
                        if callvalue() { revert(0, 0) }
                        if slt(add(calldatasize(), not(3)), 32) { revert(0, 0) }
                        let value_6 := calldataload(4)
                        mstore(0, /** @src 0:2479:2489  "msg.sender" */ caller())
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        mstore(32, 1)
                        let dataSlot_7 := keccak256(0, 64)
                        sstore(dataSlot_7, /** @src 0:2469:2500  "balanceOf[msg.sender] += amount" */ checked_add_uint256(/** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ sload(/** @src 0:2469:2500  "balanceOf[msg.sender] += amount" */ dataSlot_7), value_6))
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        sstore(0, /** @src 0:2510:2531  "totalSupply += amount" */ checked_add_uint256(/** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ sload(0), /** @src 0:2510:2531  "totalSupply += amount" */ value_6))
                        /// @src 0:2546:2586  "Transfer(address(0), msg.sender, amount)"
                        let _8 := /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ mload(64)
                        mstore(_8, value_6)
                        /// @src 0:2546:2586  "Transfer(address(0), msg.sender, amount)"
                        log3(_8, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ 32, /** @src 0:2546:2586  "Transfer(address(0), msg.sender, amount)" */ 0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ 0, /** @src 0:2479:2489  "msg.sender" */ caller())
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        return(0, 0)
                    }
                    case 0xa9059cbb {
                        if callvalue() { revert(0, 0) }
                        if slt(add(calldatasize(), not(3)), 64) { revert(0, 0) }
                        let value0_2 := abi_decode_address_3473()
                        let value_7 := calldataload(36)
                        mstore(0, /** @src 0:1734:1744  "msg.sender" */ caller())
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        mstore(32, 1)
                        let dataSlot_8 := keccak256(0, 64)
                        sstore(dataSlot_8, /** @src 0:1724:1755  "balanceOf[msg.sender] -= amount" */ checked_sub_uint256(/** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ sload(/** @src 0:1724:1755  "balanceOf[msg.sender] -= amount" */ dataSlot_8), value_7))
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        let _9 := and(value0_2, sub(shl(160, 1), 1))
                        mstore(0, _9)
                        mstore(32, 1)
                        let dataSlot_9 := keccak256(0, 64)
                        sstore(dataSlot_9, /** @src 0:1765:1795  "balanceOf[recipient] += amount" */ checked_add_uint256(/** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ sload(/** @src 0:1765:1795  "balanceOf[recipient] += amount" */ dataSlot_9), value_7))
                        /// @src 0:1810:1849  "Transfer(msg.sender, recipient, amount)"
                        let _10 := /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ mload(64)
                        mstore(_10, value_7)
                        /// @src 0:1810:1849  "Transfer(msg.sender, recipient, amount)"
                        log3(_10, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ 32, /** @src 0:1810:1849  "Transfer(msg.sender, recipient, amount)" */ 0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef, /** @src 0:1734:1744  "msg.sender" */ caller(), /** @src 0:1810:1849  "Transfer(msg.sender, recipient, amount)" */ _9)
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        let memPos_7 := mload(64)
                        mstore(memPos_7, 1)
                        return(memPos_7, 32)
                    }
                    case 0xdd62ed3e {
                        if callvalue() { revert(0, 0) }
                        if slt(add(calldatasize(), not(3)), 64) { revert(0, 0) }
                        let value0_3 := abi_decode_address_3473()
                        let value1_1 := abi_decode_address()
                        mstore(0, and(value0_3, sub(shl(160, 1), 1)))
                        mstore(32, /** @src 0:1454:1515  "mapping(address => mapping(address => uint)) public allowance" */ 2)
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        let dataSlot_10 := keccak256(0, 64)
                        /// @src 0:1454:1515  "mapping(address => mapping(address => uint)) public allowance"
                        let dataSlot_11 := /** @src -1:-1:-1 */ 0
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        mstore(/** @src -1:-1:-1 */ 0, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ and(/** @src 0:1454:1515  "mapping(address => mapping(address => uint)) public allowance" */ value1_1, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ sub(shl(160, 1), 1)))
                        mstore(0x20, /** @src 0:1454:1515  "mapping(address => mapping(address => uint)) public allowance" */ dataSlot_10)
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        dataSlot_11 := keccak256(/** @src -1:-1:-1 */ 0, /** @src 0:1347:2771  "contract ERC20 is IERC20 {..." */ 0x40)
                        let _11 := sload(/** @src 0:1454:1515  "mapping(address => mapping(address => uint)) public allowance" */ dataSlot_11)
                        /// @src 0:1347:2771  "contract ERC20 is IERC20 {..."
                        let memPos_8 := mload(64)
                        mstore(memPos_8, _11)
                        return(memPos_8, 32)
                    }
                }
                revert(0, 0)
            }
            function abi_encode_string(headStart, value0) -> tail
            {
                mstore(headStart, 32)
                let length := mload(value0)
                mstore(add(headStart, 32), length)
                mcopy(add(headStart, 64), add(value0, 32), length)
                mstore(add(add(headStart, length), 64), 0)
                tail := add(add(headStart, and(add(length, 31), not(31))), 64)
            }
            function abi_decode_address_3473() -> value
            {
                value := calldataload(4)
                if iszero(eq(value, and(value, sub(shl(160, 1), 1)))) { revert(0, 0) }
            }
            function abi_decode_address() -> value
            {
                value := calldataload(36)
                if iszero(eq(value, and(value, sub(shl(160, 1), 1)))) { revert(0, 0) }
            }
            function checked_sub_uint256(x, y) -> diff
            {
                diff := sub(x, y)
                if gt(diff, x)
                {
                    mstore(0, shl(224, 0x4e487b71))
                    mstore(4, 0x11)
                    revert(0, 0x24)
                }
            }
            function checked_add_uint256(x, y) -> sum
            {
                sum := add(x, y)
                if gt(x, sum)
                {
                    mstore(0, shl(224, 0x4e487b71))
                    mstore(4, 0x11)
                    revert(0, 0x24)
                }
            }
        }
        data ".metadata" hex"a264697066735822122050b3876a7c06489a119481ba2b8611bfb9d92d0624a61503d2b86a77af8e277164736f6c634300081c0033"
    }
}"#;

    /// Parsing the output of the print visitor as a basic integration test.
    #[test]
    fn print_visitor_works() {
        let mut printer = Printer::default();
        Object::parse(&mut Lexer::new(ERC20.into()), None)
            .unwrap()
            .accept(&mut printer);

        let mut printer2 = Printer::default();
        Object::parse(&mut Lexer::new(printer.buffer.clone()), None)
            .unwrap()
            .accept(&mut printer2);

        assert_eq!(
            printer.buffer, printer2.buffer,
            "the output from the printers must converge immediately"
        );

        assert!(
            !printer.buffer.is_empty(),
            "the printer must produce output"
        );
    }
}
