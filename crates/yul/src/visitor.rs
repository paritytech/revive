use crate::parser::{
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
    /// Visitor implementations are supposed to call this method.
    fn visit_children(&self, _ast_visitor: &mut impl AstVisitor) {}
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
