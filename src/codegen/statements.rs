use oxc_ast::ast::*;

use super::{js_number_bits, CodeGen, JS_UNDEF};

impl CodeGen {
    pub(crate) fn emit_statement(&mut self, stmt: &Statement<'_>) {
        if self.block_terminated {
            return;
        }
        match stmt {
            Statement::VariableDeclaration(decl) => self.emit_var_decl(decl),
            Statement::ExpressionStatement(es) => {
                self.emit_expression(&es.expression);
            }
            Statement::FunctionDeclaration(func) => self.emit_function_decl(func),
            Statement::IfStatement(s) => self.emit_if(s),
            Statement::WhileStatement(s) => self.emit_while(s),
            Statement::ForStatement(s) => self.emit_for(s),
            Statement::ForOfStatement(s) => self.emit_for_of(s),
            Statement::ReturnStatement(s) => self.emit_return(s),
            Statement::ThrowStatement(s) => self.emit_throw(s),
            Statement::BlockStatement(s) => {
                self.push_scope();
                for inner in &s.body {
                    self.emit_statement(inner);
                }
                self.pop_scope();
            }
            Statement::EmptyStatement(_) => {}
            Statement::TryStatement(s) => self.emit_try(s),
            _ => {
                // Unsupported statement — skip with warning
            }
        }
    }

    pub(crate) fn emit_var_decl(&mut self, decl: &VariableDeclaration<'_>) {
        for declarator in &decl.declarations {
            let name = match &declarator.id {
                BindingPattern::BindingIdentifier(id) => id.name.as_str().to_string(),
                _ => panic!("unsupported variable binding pattern"),
            };
            let alloca = self.declare_var(&name);
            let init_val = if let Some(init) = &declarator.init {
                self.emit_expression(init)
            } else {
                format!("{}", JS_UNDEF)
            };
            self.emit(&format!(
                "  store i64 {}, ptr {}, align 8",
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

        self.emit_label(&then_label);
        self.emit_statement(&stmt.consequent);
        self.emit_br(&end_label);

        if let Some(alt) = &stmt.alternate {
            self.emit_label(&else_label);
            self.emit_statement(alt);
            self.emit_br(&end_label);
        }

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

        if let Some(init) = &stmt.init {
            match init {
                ForStatementInit::VariableDeclaration(decl) => self.emit_var_decl(decl),
                _ => {
                    if let Some(expr) = init.as_expression() {
                        self.emit_expression(expr);
                    }
                }
            }
        }

        let cond_label = self.fresh_label("for.cond");
        let body_label = self.fresh_label("for.body");
        let update_label = self.fresh_label("for.update");
        let end_label = self.fresh_label("for.end");

        self.emit_br(&cond_label);
        self.emit_label(&cond_label);
        if let Some(test) = &stmt.test {
            let cond = self.emit_expression(test);
            let cond_bool = self.to_bool(&cond);
            self.emit_cond_br(&cond_bool, &body_label, &end_label);
        } else {
            self.emit_br(&body_label);
        }

        self.emit_label(&body_label);
        self.emit_statement(&stmt.body);
        self.emit_br(&update_label);

        self.emit_label(&update_label);
        if let Some(update) = &stmt.update {
            self.emit_expression(update);
        }
        self.emit_br(&cond_label);

        self.emit_label(&end_label);
        self.pop_scope();
    }

    fn emit_for_of(&mut self, stmt: &ForOfStatement<'_>) {
        self.push_scope();

        let iterable = self.emit_expression(&stmt.right);
        let len_key = self.emit_string_const("length");
        let len_val = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
            len_val, iterable, len_key
        ));

        // Index counter
        let idx_alloca = {
            let m = format!("%forof.idx.{}", self.var_counter);
            self.var_counter += 1;
            self.emit(&format!("  {} = alloca i64, align 8", m));
            self.emit(&format!(
                "  store i64 {}, ptr {}, align 8",
                js_number_bits(0.0),
                m
            ));
            m
        };

        // Declare the iteration variable
        let iter_var_name = match &stmt.left {
            ForStatementLeft::VariableDeclaration(decl) => {
                match &decl.declarations[0].id {
                    BindingPattern::BindingIdentifier(id) => id.name.as_str().to_string(),
                    _ => panic!("unsupported for-of variable pattern"),
                }
            }
            _ => panic!("unsupported for-of left-hand side"),
        };
        let iter_alloca = self.declare_var(&iter_var_name);

        let cond_label = self.fresh_label("forof.cond");
        let body_label = self.fresh_label("forof.body");
        let end_label = self.fresh_label("forof.end");

        self.emit_br(&cond_label);
        self.emit_label(&cond_label);
        let idx = self.fresh_reg();
        self.emit(&format!(
            "  {} = load i64, ptr {}, align 8",
            idx, idx_alloca
        ));
        let cmp = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_lt(i64 {}, i64 {})",
            cmp, idx, len_val
        ));
        let cmp_bool = self.to_bool(&cmp);
        self.emit_cond_br(&cmp_bool, &body_label, &end_label);

        self.emit_label(&body_label);
        let elem = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
            elem, iterable, idx
        ));
        self.emit(&format!(
            "  store i64 {}, ptr {}, align 8",
            elem, iter_alloca
        ));

        self.emit_statement(&stmt.body);

        // Increment index
        let one = js_number_bits(1.0);
        let next_idx = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_add(i64 {}, i64 {})",
            next_idx, idx, one
        ));
        self.emit(&format!(
            "  store i64 {}, ptr {}, align 8",
            next_idx, idx_alloca
        ));
        self.emit_br(&cond_label);

        self.emit_label(&end_label);
        self.pop_scope();
    }

    fn emit_return(&mut self, stmt: &ReturnStatement<'_>) {
        let val = if let Some(arg) = &stmt.argument {
            self.emit_expression(arg)
        } else {
            format!("{}", JS_UNDEF)
        };
        if self.is_main {
            self.emit("  ret i32 0");
        } else {
            self.emit(&format!("  ret i64 {}", val));
        }
        self.block_terminated = true;
    }

    fn emit_throw(&mut self, stmt: &ThrowStatement<'_>) {
        let val = self.emit_expression(&stmt.argument);
        self.emit(&format!("  call void @js_throw(i64 {})", val));
        self.emit("  unreachable");
        self.block_terminated = true;
    }

    fn emit_try(&mut self, stmt: &TryStatement<'_>) {
        // Basic try/catch is complex with setjmp — for now, just execute the try body
        // and skip catch. Errors will still exit via js_throw.
        for s in &stmt.block.body {
            self.emit_statement(s);
        }
        // TODO: implement proper try/catch with setjmp/longjmp
    }
}
