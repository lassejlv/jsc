use std::collections::HashMap;
use std::fmt::Write as FmtWrite;

use oxc_ast::ast::*;
use oxc_syntax::operator::{
    BinaryOperator, LogicalOperator, UnaryOperator, UpdateOperator,
};

pub struct CodeGen {
    globals: String,
    functions: Vec<String>,
    current_fn: String,
    next_reg: u32,
    next_label: u32,
    next_str: u32,
    scopes: Vec<HashMap<String, String>>,
    var_counter: u32,
    string_constants: Vec<(String, String, usize)>,
    block_terminated: bool,
    current_block: String,
    is_main: bool,
}

impl CodeGen {
    fn new() -> Self {
        Self {
            globals: String::new(),
            functions: Vec::new(),
            current_fn: String::new(),
            next_reg: 0,
            next_label: 0,
            next_str: 0,
            scopes: Vec::new(),
            var_counter: 0,
            string_constants: Vec::new(),
            block_terminated: false,
            current_block: "entry".to_string(),
            is_main: false,
        }
    }

    pub fn compile(program: &Program<'_>) -> String {
        let mut cg = Self::new();

        // First pass: emit user-defined functions
        for stmt in &program.body {
            if let Statement::FunctionDeclaration(func) = stmt {
                cg.emit_function_decl(func);
            }
        }

        // Second pass: emit top-level code into main()
        cg.begin_main();
        for stmt in &program.body {
            if !matches!(stmt, Statement::FunctionDeclaration(_)) {
                cg.emit_statement(stmt);
            }
        }
        cg.end_main();

        cg.finalize()
    }

    // ---- Helpers ----

    fn fresh_reg(&mut self) -> String {
        let r = format!("%t{}", self.next_reg);
        self.next_reg += 1;
        r
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        let l = format!("{}.{}", prefix, self.next_label);
        self.next_label += 1;
        l
    }

    fn emit(&mut self, line: &str) {
        writeln!(self.current_fn, "{}", line).unwrap();
    }

    fn emit_label(&mut self, label: &str) {
        writeln!(self.current_fn, "{}:", label).unwrap();
        self.current_block = label.to_string();
        self.block_terminated = false;
    }

    fn emit_br(&mut self, label: &str) {
        if !self.block_terminated {
            self.emit(&format!("  br label %{}", label));
            self.block_terminated = true;
        }
    }

    fn emit_cond_br(&mut self, cond: &str, then_l: &str, else_l: &str) {
        if !self.block_terminated {
            self.emit(&format!(
                "  br i1 {}, label %{}, label %{}",
                cond, then_l, else_l
            ));
            self.block_terminated = true;
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare_var(&mut self, name: &str) -> String {
        let mangled = format!("%js.{}.{}", name, self.var_counter);
        self.var_counter += 1;
        self.emit(&format!("  {} = alloca double, align 8", mangled));
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), mangled.clone());
        }
        mangled
    }

    fn lookup_var(&self, name: &str) -> &str {
        for scope in self.scopes.iter().rev() {
            if let Some(reg) = scope.get(name) {
                return reg;
            }
        }
        panic!("undefined variable: {}", name);
    }

    fn add_string_constant(&mut self, s: &str) -> String {
        let name = format!("@.str.{}", self.next_str);
        self.next_str += 1;
        let mut escaped = String::new();
        let mut byte_len = 0usize;
        for c in s.chars() {
            match c {
                '\n' => {
                    escaped.push_str("\\0A");
                    byte_len += 1;
                }
                '\r' => {
                    escaped.push_str("\\0D");
                    byte_len += 1;
                }
                '\t' => {
                    escaped.push_str("\\09");
                    byte_len += 1;
                }
                '\\' => {
                    escaped.push_str("\\5C");
                    byte_len += 1;
                }
                '"' => {
                    escaped.push_str("\\22");
                    byte_len += 1;
                }
                c if c.is_ascii() && !c.is_ascii_control() => {
                    escaped.push(c);
                    byte_len += 1;
                }
                c => {
                    let mut buf = [0u8; 4];
                    let encoded = c.encode_utf8(&mut buf);
                    for b in encoded.bytes() {
                        write!(escaped, "\\{:02X}", b).unwrap();
                        byte_len += 1;
                    }
                }
            }
        }
        byte_len += 1; // null terminator
        self.string_constants
            .push((name.clone(), escaped, byte_len));
        name
    }

    fn format_f64(v: f64) -> String {
        if v == 0.0 {
            if v.is_sign_negative() {
                return "-0.0".to_string();
            }
            return "0.0".to_string();
        }
        if v.fract() == 0.0 && v.abs() < 1e15 {
            format!("{:.1}", v)
        } else {
            format!("{:e}", v)
        }
    }

    /// Convert a double value to an i1 boolean (true if != 0.0)
    fn to_bool(&mut self, val: &str) -> String {
        let reg = self.fresh_reg();
        self.emit(&format!("  {} = fcmp une double {}, 0.0", reg, val));
        reg
    }

    // ---- Main function wrapper ----

    fn begin_main(&mut self) {
        self.current_fn = String::new();
        self.scopes = vec![HashMap::new()];
        self.block_terminated = false;
        self.current_block = "entry".to_string();
        self.is_main = true;
        self.emit("define i32 @main() {");
        self.emit("entry:");
    }

    fn end_main(&mut self) {
        if !self.block_terminated {
            self.emit("  ret i32 0");
        }
        self.emit("}");
        self.functions.push(std::mem::take(&mut self.current_fn));
        self.is_main = false;
    }

    // ---- Function declarations ----

    fn emit_function_decl(&mut self, func: &Function<'_>) {
        let name = func
            .id
            .as_ref()
            .expect("function must have a name")
            .name
            .as_str();

        // Save state
        let saved_fn = std::mem::take(&mut self.current_fn);
        let saved_scopes = std::mem::take(&mut self.scopes);
        let saved_terminated = self.block_terminated;
        let saved_block = std::mem::take(&mut self.current_block);
        let saved_is_main = self.is_main;

        // New function context
        self.scopes = vec![HashMap::new()];
        self.block_terminated = false;
        self.current_block = "entry".to_string();
        self.is_main = false;

        // Build parameter list
        let params = &func.params;
        let mut param_names = Vec::new();
        let mut param_ir = Vec::new();
        for param in &params.items {
            let pname = match &param.pattern {
                BindingPattern::BindingIdentifier(id) => id.name.as_str().to_string(),
                _ => panic!("unsupported parameter pattern"),
            };
            param_ir.push(format!("double %param.{}", pname));
            param_names.push(pname);
        }

        self.emit(&format!(
            "define double @{}({}) {{",
            name,
            param_ir.join(", ")
        ));
        self.emit("entry:");

        // Alloca + store for each parameter
        for pname in &param_names {
            let alloca = self.declare_var(pname);
            self.emit(&format!(
                "  store double %param.{}, ptr {}, align 8",
                pname, alloca
            ));
        }

        // Emit body
        if let Some(body) = &func.body {
            for stmt in &body.statements {
                if self.block_terminated {
                    break;
                }
                self.emit_statement(stmt);
            }
        }

        // Default return if not terminated
        if !self.block_terminated {
            self.emit("  ret double 0.0");
        }
        self.emit("}");

        self.functions.push(std::mem::take(&mut self.current_fn));

        // Restore state
        self.current_fn = saved_fn;
        self.scopes = saved_scopes;
        self.block_terminated = saved_terminated;
        self.current_block = saved_block;
        self.is_main = saved_is_main;
    }

    // ---- Output assembly ----

    fn finalize(&self) -> String {
        let mut output = String::new();
        writeln!(output, "; Generated by js-compiler").unwrap();
        writeln!(output).unwrap();
        writeln!(output, "declare i32 @printf(ptr, ...)").unwrap();
        writeln!(output).unwrap();
        writeln!(
            output,
            "@.fmt.num = private unnamed_addr constant [7 x i8] c\"%.17g\\0A\\00\""
        )
        .unwrap();
        writeln!(
            output,
            "@.fmt.str = private unnamed_addr constant [4 x i8] c\"%s\\0A\\00\""
        )
        .unwrap();
        writeln!(output).unwrap();

        // User string constants
        for (name, escaped, len) in &self.string_constants {
            writeln!(
                output,
                "{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
                name, len, escaped
            )
            .unwrap();
        }
        if !self.string_constants.is_empty() {
            writeln!(output).unwrap();
        }

        // Extra globals
        if !self.globals.is_empty() {
            write!(output, "{}", self.globals).unwrap();
            writeln!(output).unwrap();
        }

        // Functions
        for func in &self.functions {
            writeln!(output, "{}", func).unwrap();
        }

        output
    }

    // ---- Statement emission ----

    fn emit_statement(&mut self, stmt: &Statement<'_>) {
        if self.block_terminated {
            return;
        }
        match stmt {
            Statement::VariableDeclaration(decl) => self.emit_var_decl(decl),
            Statement::ExpressionStatement(es) => {
                self.emit_expression(&es.expression);
            }
            Statement::FunctionDeclaration(func) => {
                // Nested function declaration — emit as top-level LLVM function
                self.emit_function_decl(func);
            }
            Statement::IfStatement(s) => self.emit_if(s),
            Statement::WhileStatement(s) => self.emit_while(s),
            Statement::ForStatement(s) => self.emit_for(s),
            Statement::ReturnStatement(s) => self.emit_return(s),
            Statement::BlockStatement(s) => {
                self.push_scope();
                for inner in &s.body {
                    self.emit_statement(inner);
                }
                self.pop_scope();
            }
            Statement::EmptyStatement(_) => {}
            _ => panic!(
                "unsupported statement type"
            ),
        }
    }

    fn emit_var_decl(&mut self, decl: &VariableDeclaration<'_>) {
        for declarator in &decl.declarations {
            let name = match &declarator.id {
                BindingPattern::BindingIdentifier(id) => id.name.as_str().to_string(),
                _ => panic!("unsupported variable binding pattern"),
            };
            let alloca = self.declare_var(&name);
            let init_val = if let Some(init) = &declarator.init {
                self.emit_expression(init)
            } else {
                "0.0".to_string()
            };
            self.emit(&format!(
                "  store double {}, ptr {}, align 8",
                init_val, alloca
            ));
        }
    }

    fn emit_if(&mut self, stmt: &IfStatement<'_>) {
        let cond = self.emit_expression(&stmt.test);
        let cond_bool = self.to_bool(&cond);

        let then_label = self.fresh_label("if.then");
        let else_label = self.fresh_label("if.else");
        let end_label = self.fresh_label("if.end");

        if stmt.alternate.is_some() {
            self.emit_cond_br(&cond_bool, &then_label, &else_label);
        } else {
            self.emit_cond_br(&cond_bool, &then_label, &end_label);
        }

        // Then block
        self.emit_label(&then_label);
        self.emit_statement(&stmt.consequent);
        self.emit_br(&end_label);

        // Else block (if present)
        if let Some(alt) = &stmt.alternate {
            self.emit_label(&else_label);
            self.emit_statement(alt);
            self.emit_br(&end_label);
        }

        // Merge point
        self.emit_label(&end_label);
    }

    fn emit_while(&mut self, stmt: &WhileStatement<'_>) {
        let cond_label = self.fresh_label("while.cond");
        let body_label = self.fresh_label("while.body");
        let end_label = self.fresh_label("while.end");

        self.emit_br(&cond_label);

        self.emit_label(&cond_label);
        let cond = self.emit_expression(&stmt.test);
        let cond_bool = self.to_bool(&cond);
        self.emit_cond_br(&cond_bool, &body_label, &end_label);

        self.emit_label(&body_label);
        self.emit_statement(&stmt.body);
        self.emit_br(&cond_label);

        self.emit_label(&end_label);
    }

    fn emit_for(&mut self, stmt: &ForStatement<'_>) {
        self.push_scope();

        // Init
        if let Some(init) = &stmt.init {
            match init {
                ForStatementInit::VariableDeclaration(decl) => self.emit_var_decl(decl),
                _ => {
                    // Try to handle expression-based init
                    if let Some(expr) = init.as_expression() {
                        self.emit_expression(expr);
                    } else {
                        panic!("unsupported for-loop init");
                    }
                }
            }
        }

        let cond_label = self.fresh_label("for.cond");
        let body_label = self.fresh_label("for.body");
        let update_label = self.fresh_label("for.update");
        let end_label = self.fresh_label("for.end");

        self.emit_br(&cond_label);

        // Condition
        self.emit_label(&cond_label);
        if let Some(test) = &stmt.test {
            let cond = self.emit_expression(test);
            let cond_bool = self.to_bool(&cond);
            self.emit_cond_br(&cond_bool, &body_label, &end_label);
        } else {
            self.emit_br(&body_label); // infinite loop
        }

        // Body
        self.emit_label(&body_label);
        self.emit_statement(&stmt.body);
        self.emit_br(&update_label);

        // Update
        self.emit_label(&update_label);
        if let Some(update) = &stmt.update {
            self.emit_expression(update);
        }
        self.emit_br(&cond_label);

        // End
        self.emit_label(&end_label);
        self.pop_scope();
    }

    fn emit_return(&mut self, stmt: &ReturnStatement<'_>) {
        let val = if let Some(arg) = &stmt.argument {
            self.emit_expression(arg)
        } else {
            "0.0".to_string()
        };
        if self.is_main {
            self.emit("  ret i32 0");
        } else {
            self.emit(&format!("  ret double {}", val));
        }
        self.block_terminated = true;
    }

    // ---- Expression emission ----

    fn emit_expression(&mut self, expr: &Expression<'_>) -> String {
        match expr {
            Expression::NumericLiteral(lit) => Self::format_f64(lit.value),
            Expression::BooleanLiteral(lit) => {
                if lit.value {
                    "1.0".to_string()
                } else {
                    "0.0".to_string()
                }
            }
            Expression::Identifier(id) => {
                let alloca = self.lookup_var(id.name.as_str()).to_string();
                let reg = self.fresh_reg();
                self.emit(&format!("  {} = load double, ptr {}, align 8", reg, alloca));
                reg
            }
            Expression::BinaryExpression(be) => self.emit_binary(be),
            Expression::UnaryExpression(ue) => self.emit_unary(ue),
            Expression::LogicalExpression(le) => self.emit_logical(le),
            Expression::AssignmentExpression(ae) => self.emit_assignment(ae),
            Expression::CallExpression(ce) => self.emit_call(ce),
            Expression::UpdateExpression(ue) => self.emit_update(ue),
            Expression::ParenthesizedExpression(pe) => self.emit_expression(&pe.expression),
            Expression::StringLiteral(_) => {
                panic!("string literals are only supported as console.log arguments")
            }
            _ => panic!("unsupported expression type"),
        }
    }

    fn emit_binary(&mut self, expr: &BinaryExpression<'_>) -> String {
        let left = self.emit_expression(&expr.left);
        let right = self.emit_expression(&expr.right);
        let reg = self.fresh_reg();

        match expr.operator {
            BinaryOperator::Addition => {
                self.emit(&format!("  {} = fadd double {}, {}", reg, left, right));
                reg
            }
            BinaryOperator::Subtraction => {
                self.emit(&format!("  {} = fsub double {}, {}", reg, left, right));
                reg
            }
            BinaryOperator::Multiplication => {
                self.emit(&format!("  {} = fmul double {}, {}", reg, left, right));
                reg
            }
            BinaryOperator::Division => {
                self.emit(&format!("  {} = fdiv double {}, {}", reg, left, right));
                reg
            }
            BinaryOperator::Remainder => {
                self.emit(&format!("  {} = frem double {}, {}", reg, left, right));
                reg
            }
            // Comparisons: result is i1, convert to double
            op @ (BinaryOperator::LessThan
            | BinaryOperator::GreaterThan
            | BinaryOperator::LessEqualThan
            | BinaryOperator::GreaterEqualThan
            | BinaryOperator::Equality
            | BinaryOperator::Inequality
            | BinaryOperator::StrictEquality
            | BinaryOperator::StrictInequality) => {
                let pred = match op {
                    BinaryOperator::LessThan => "olt",
                    BinaryOperator::GreaterThan => "ogt",
                    BinaryOperator::LessEqualThan => "ole",
                    BinaryOperator::GreaterEqualThan => "oge",
                    BinaryOperator::Equality | BinaryOperator::StrictEquality => "oeq",
                    BinaryOperator::Inequality | BinaryOperator::StrictInequality => "une",
                    _ => unreachable!(),
                };
                let cmp_reg = self.fresh_reg();
                self.emit(&format!(
                    "  {} = fcmp {} double {}, {}",
                    cmp_reg, pred, left, right
                ));
                self.emit(&format!("  {} = uitofp i1 {} to double", reg, cmp_reg));
                reg
            }
            _ => panic!("unsupported binary operator: {:?}", expr.operator),
        }
    }

    fn emit_unary(&mut self, expr: &UnaryExpression<'_>) -> String {
        let operand = self.emit_expression(&expr.argument);
        match expr.operator {
            UnaryOperator::UnaryNegation => {
                let reg = self.fresh_reg();
                self.emit(&format!("  {} = fneg double {}", reg, operand));
                reg
            }
            UnaryOperator::LogicalNot => {
                let cmp = self.fresh_reg();
                self.emit(&format!("  {} = fcmp oeq double {}, 0.0", cmp, operand));
                let reg = self.fresh_reg();
                self.emit(&format!("  {} = uitofp i1 {} to double", reg, cmp));
                reg
            }
            UnaryOperator::UnaryPlus => operand, // no-op for numbers
            _ => panic!("unsupported unary operator: {:?}", expr.operator),
        }
    }

    fn emit_logical(&mut self, expr: &LogicalExpression<'_>) -> String {
        let left = self.emit_expression(&expr.left);
        let left_bool = self.to_bool(&left);
        let left_block = self.current_block.clone();

        let rhs_label = self.fresh_label("logic.rhs");
        let end_label = self.fresh_label("logic.end");

        match expr.operator {
            LogicalOperator::And => {
                // Short-circuit: if left is falsy, result is left
                self.emit_cond_br(&left_bool, &rhs_label, &end_label);
            }
            LogicalOperator::Or => {
                // Short-circuit: if left is truthy, result is left
                self.emit_cond_br(&left_bool, &end_label, &rhs_label);
            }
            _ => panic!("unsupported logical operator: {:?}", expr.operator),
        }

        // Evaluate RHS
        self.emit_label(&rhs_label);
        let right = self.emit_expression(&expr.right);
        let rhs_block = self.current_block.clone();
        self.emit_br(&end_label);

        // Merge with phi
        self.emit_label(&end_label);
        let result = self.fresh_reg();
        self.emit(&format!(
            "  {} = phi double [ {}, %{} ], [ {}, %{} ]",
            result, left, left_block, right, rhs_block
        ));
        result
    }

    fn emit_assignment(&mut self, expr: &AssignmentExpression<'_>) -> String {
        let val = self.emit_expression(&expr.right);
        match &expr.left {
            AssignmentTarget::AssignmentTargetIdentifier(id) => {
                let alloca = self.lookup_var(id.name.as_str()).to_string();
                self.emit(&format!("  store double {}, ptr {}, align 8", val, alloca));
            }
            _ => panic!("unsupported assignment target"),
        }
        val
    }

    fn emit_call(&mut self, expr: &CallExpression<'_>) -> String {
        // Check for console.log
        if self.is_console_log(expr) {
            self.emit_console_log(&expr.arguments);
            return "0.0".to_string();
        }

        // Regular function call
        let name = match &expr.callee {
            Expression::Identifier(id) => id.name.as_str().to_string(),
            _ => panic!("unsupported: only direct function calls (e.g., `foo()`) are supported"),
        };

        let mut arg_vals = Vec::new();
        for arg in &expr.arguments {
            let val = self.emit_call_arg(arg);
            arg_vals.push(format!("double {}", val));
        }

        let reg = self.fresh_reg();
        self.emit(&format!(
            "  {} = call double @{}({})",
            reg,
            name,
            arg_vals.join(", ")
        ));
        reg
    }

    fn is_console_log(&self, expr: &CallExpression<'_>) -> bool {
        if let Expression::StaticMemberExpression(sme) = &expr.callee {
            if let Expression::Identifier(obj) = &sme.object {
                return obj.name.as_str() == "console" && sme.property.name.as_str() == "log";
            }
        }
        false
    }

    fn emit_console_log(&mut self, args: &[Argument<'_>]) {
        for arg in args {
            // Check if the argument is a string literal
            if let Argument::StringLiteral(lit) = arg {
                let global_name = self.add_string_constant(lit.value.as_str());
                let reg = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i32 (ptr, ...) @printf(ptr @.fmt.str, ptr {})",
                    reg, global_name
                ));
            } else {
                // Emit as numeric expression
                let val = self.emit_call_arg(arg);
                let reg = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i32 (ptr, ...) @printf(ptr @.fmt.num, double {})",
                    reg, val
                ));
            }
        }
    }

    fn emit_call_arg(&mut self, arg: &Argument<'_>) -> String {
        match arg {
            Argument::NumericLiteral(lit) => Self::format_f64(lit.value),
            Argument::BooleanLiteral(lit) => {
                if lit.value {
                    "1.0".to_string()
                } else {
                    "0.0".to_string()
                }
            }
            Argument::Identifier(id) => {
                let alloca = self.lookup_var(id.name.as_str()).to_string();
                let reg = self.fresh_reg();
                self.emit(&format!("  {} = load double, ptr {}, align 8", reg, alloca));
                reg
            }
            Argument::BinaryExpression(be) => self.emit_binary(be),
            Argument::UnaryExpression(ue) => self.emit_unary(ue),
            Argument::LogicalExpression(le) => self.emit_logical(le),
            Argument::CallExpression(ce) => self.emit_call(ce),
            Argument::ParenthesizedExpression(pe) => self.emit_expression(&pe.expression),
            Argument::AssignmentExpression(ae) => self.emit_assignment(ae),
            Argument::UpdateExpression(ue) => self.emit_update(ue),
            Argument::StringLiteral(_) => {
                panic!("string literals can only be used directly in console.log")
            }
            _ => panic!("unsupported argument type"),
        }
    }

    fn emit_update(&mut self, expr: &UpdateExpression<'_>) -> String {
        let var_name = match &expr.argument {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                id.name.as_str().to_string()
            }
            _ => panic!("unsupported update expression target"),
        };

        let alloca = self.lookup_var(&var_name).to_string();
        let old_val = self.fresh_reg();
        self.emit(&format!(
            "  {} = load double, ptr {}, align 8",
            old_val, alloca
        ));

        let new_val = self.fresh_reg();
        match expr.operator {
            UpdateOperator::Increment => {
                self.emit(&format!("  {} = fadd double {}, 1.0", new_val, old_val));
            }
            UpdateOperator::Decrement => {
                self.emit(&format!("  {} = fsub double {}, 1.0", new_val, old_val));
            }
        }

        self.emit(&format!(
            "  store double {}, ptr {}, align 8",
            new_val, alloca
        ));

        // Prefix (++i) returns new value, postfix (i++) returns old value
        if expr.prefix {
            new_val
        } else {
            old_val
        }
    }
}
