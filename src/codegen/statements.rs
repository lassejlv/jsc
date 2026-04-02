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
            Statement::SwitchStatement(s) => self.emit_switch(s),
            Statement::DoWhileStatement(s) => self.emit_do_while(s),
            Statement::ForInStatement(s) => self.emit_for_in(s),
            Statement::BreakStatement(_) => {
                if let Some((break_label, _)) = self.loop_stack.last().cloned() {
                    self.emit_br(&break_label);
                    self.block_terminated = true;
                }
            }
            Statement::ContinueStatement(_) => {
                if let Some((_, continue_label)) = self.loop_stack.last().cloned() {
                    self.emit_br(&continue_label);
                    self.block_terminated = true;
                }
            }
            Statement::LabeledStatement(s) => {
                self.emit_statement(&s.body);
            }
            Statement::ImportDeclaration(decl) => self.emit_import(decl),
            Statement::ExportNamedDeclaration(decl) => self.emit_export_named(decl),
            Statement::ExportDefaultDeclaration(decl) => self.emit_export_default(decl),
            Statement::ExportAllDeclaration(_) => {
                // TODO: export * from "..."
            }
            // TypeScript: skip type-only declarations
            Statement::TSTypeAliasDeclaration(_) => {}
            Statement::TSInterfaceDeclaration(_) => {}
            Statement::TSModuleDeclaration(_) => {}
            Statement::TSGlobalDeclaration(_) => {}
            Statement::TSImportEqualsDeclaration(_) => {}
            Statement::TSExportAssignment(_) => {}
            Statement::TSNamespaceExportDeclaration(_) => {}
            // TypeScript: compile enums to objects
            Statement::TSEnumDeclaration(e) => self.emit_ts_enum(e),
            _ => {
                // Unsupported statement — skip
            }
        }
    }

    pub(crate) fn emit_var_decl(&mut self, decl: &VariableDeclaration<'_>) {
        for declarator in &decl.declarations {
            let init_val = if let Some(init) = &declarator.init {
                self.emit_expression(init)
            } else {
                format!("{}", JS_UNDEF)
            };
            self.emit_binding_pattern(&declarator.id, &init_val);
        }
    }

    /// Recursively destructure a binding pattern, assigning from `value_reg`.
    pub(crate) fn emit_binding_pattern(&mut self, pattern: &BindingPattern, value_reg: &str) {
        match pattern {
            BindingPattern::BindingIdentifier(id) => {
                let alloca = self.declare_var(id.name.as_str());
                self.emit(&format!(
                    "  store i64 {}, ptr {}, align 8",
                    value_reg, alloca
                ));
            }
            BindingPattern::ObjectPattern(op) => {
                for prop in &op.properties {
                    let (key_name, binding) = match &prop.key {
                        PropertyKey::StaticIdentifier(id) => {
                            (id.name.as_str().to_string(), &prop.value)
                        }
                        PropertyKey::StringLiteral(s) => {
                            (s.value.as_str().to_string(), &prop.value)
                        }
                        _ => continue,
                    };
                    let key_reg = self.emit_string_const(&key_name);
                    let prop_val = self.fresh_reg();
                    self.emit(&format!(
                        "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
                        prop_val, value_reg, key_reg
                    ));
                    self.emit_binding_pattern(binding, &prop_val);
                }
                if let Some(rest) = &op.rest {
                    // Rest element: bind remaining properties (simplified — binds full object)
                    self.emit_binding_pattern(&rest.argument, value_reg);
                }
            }
            BindingPattern::ArrayPattern(ap) => {
                for (i, elem) in ap.elements.iter().enumerate() {
                    if let Some(binding) = elem {
                        let idx_reg = format!("{}", js_number_bits(i as f64));
                        let elem_val = self.fresh_reg();
                        self.emit(&format!(
                            "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
                            elem_val, value_reg, idx_reg
                        ));
                        self.emit_binding_pattern(binding, &elem_val);
                    }
                    // None = elision, skip
                }
                if let Some(rest) = &ap.rest {
                    // Rest element: create array from remaining elements
                    let start_idx = ap.elements.len();
                    let rest_arr = self.fresh_reg();
                    self.emit(&format!("  {} = call i64 @js_array_new()", rest_arr));

                    let len_key = self.emit_string_const("length");
                    let len_val = self.fresh_reg();
                    self.emit(&format!(
                        "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
                        len_val, value_reg, len_key
                    ));

                    // Loop from start_idx to length
                    let idx_alloca = {
                        let m = format!("%rest.idx.{}", self.var_counter);
                        self.var_counter += 1;
                        self.emit(&format!("  {} = alloca i64, align 8", m));
                        self.emit(&format!(
                            "  store i64 {}, ptr {}, align 8",
                            js_number_bits(start_idx as f64), m
                        ));
                        m
                    };

                    let cond_label = self.fresh_label("rest.cond");
                    let body_label = self.fresh_label("rest.body");
                    let end_label = self.fresh_label("rest.end");

                    self.emit_br(&cond_label);
                    self.emit_label(&cond_label);
                    let idx = self.fresh_reg();
                    self.emit(&format!("  {} = load i64, ptr {}, align 8", idx, idx_alloca));
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
                        elem, value_reg, idx
                    ));
                    self.emit(&format!(
                        "  call i64 @js_array_push_val(i64 {}, i64 {})",
                        rest_arr, elem
                    ));
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
                    self.emit_binding_pattern(&rest.argument, &rest_arr);
                }
            }
            BindingPattern::AssignmentPattern(ap) => {
                // Default value: if value is undefined, use the default
                let is_undef = self.fresh_reg();
                self.emit(&format!(
                    "  {} = icmp eq i64 {}, {}",
                    is_undef, value_reg, JS_UNDEF
                ));
                let default_label = self.fresh_label("default.yes");
                let no_default_label = self.fresh_label("default.no");
                let merge_label = self.fresh_label("default.merge");

                self.emit_cond_br(&is_undef, &default_label, &no_default_label);

                self.emit_label(&default_label);
                let default_val = self.emit_expression(&ap.right);
                let default_block = self.current_block.clone();
                self.emit_br(&merge_label);

                self.emit_label(&no_default_label);
                let no_default_block = self.current_block.clone();
                self.emit_br(&merge_label);

                self.emit_label(&merge_label);
                let final_val = self.fresh_reg();
                self.emit(&format!(
                    "  {} = phi i64 [ {}, %{} ], [ {}, %{} ]",
                    final_val, default_val, default_block, value_reg, no_default_block
                ));
                self.emit_binding_pattern(&ap.left, &final_val);
            }
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
        self.loop_stack.push((end_label.clone(), cond_label.clone()));
        self.emit_statement(&stmt.body);
        self.loop_stack.pop();
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
        self.loop_stack.push((end_label.clone(), update_label.clone()));
        self.emit_statement(&stmt.body);
        self.loop_stack.pop();
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

        // Get the binding pattern from the for-of left-hand side
        let decl = match &stmt.left {
            ForStatementLeft::VariableDeclaration(decl) => decl,
            _ => panic!("unsupported for-of left-hand side"),
        };
        let is_simple = matches!(
            &decl.declarations[0].id,
            BindingPattern::BindingIdentifier(_)
        );

        // Pre-declare simple identifier variables before the loop
        if let BindingPattern::BindingIdentifier(id) = &decl.declarations[0].id {
            self.declare_var(id.name.as_str());
        }

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

        // Assign element via binding pattern (supports destructuring)
        if is_simple {
            if let BindingPattern::BindingIdentifier(id) = &decl.declarations[0].id {
                let alloca = self.lookup_var(id.name.as_str()).to_string();
                self.emit(&format!(
                    "  store i64 {}, ptr {}, align 8",
                    elem, alloca
                ));
            }
        } else {
            self.emit_binding_pattern(&decl.declarations[0].id, &elem);
        }

        let update_label = self.fresh_label("forof.update");
        self.loop_stack.push((end_label.clone(), update_label.clone()));
        self.emit_statement(&stmt.body);
        self.loop_stack.pop();
        self.emit_br(&update_label);

        // Increment index
        self.emit_label(&update_label);
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
            self.emit("  call void @js_run_event_loop()");
            self.emit("  ret i32 0");
        } else if self.is_async {
            let wrapped = self.fresh_reg();
            self.emit(&format!(
                "  {} = call i64 @js_async_return(i64 {})",
                wrapped, val
            ));
            self.emit(&format!("  ret i64 {}", wrapped));
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
        let try_label = self.fresh_label("try.body");
        let catch_label = self.fresh_label("try.catch");
        let finally_label = self.fresh_label("try.finally");
        let end_label = self.fresh_label("try.end");

        let has_catch = stmt.handler.is_some();
        let has_finally = stmt.finalizer.is_some();
        let after_try = if has_finally { &finally_label } else { &end_label };
        let after_catch = if has_finally { &finally_label } else { &end_label };

        // Get jmp_buf and call setjmp
        let buf_ptr = self.fresh_reg();
        self.emit(&format!("  {} = call ptr @js_try_get_buf()", buf_ptr));
        let setjmp_ret = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i32 @_setjmp(ptr {})",
            setjmp_ret, buf_ptr
        ));
        let is_catch = self.fresh_reg();
        self.emit(&format!(
            "  {} = icmp ne i32 {}, 0",
            is_catch, setjmp_ret
        ));

        if has_catch {
            self.emit_cond_br(&is_catch, &catch_label, &try_label);
        } else {
            self.emit_cond_br(&is_catch, after_try, &try_label);
        }

        // Try body
        self.emit_label(&try_label);
        self.push_scope();
        for s in &stmt.block.body {
            if self.block_terminated {
                break;
            }
            self.emit_statement(s);
        }
        self.pop_scope();
        if !self.block_terminated {
            self.emit("  call void @js_try_exit()");
        }
        self.emit_br(after_try);

        // Catch body
        if let Some(handler) = &stmt.handler {
            self.emit_label(&catch_label);
            self.block_terminated = false;
            self.push_scope();

            // Get the thrown error value
            let err_val = self.fresh_reg();
            self.emit(&format!("  {} = call i64 @js_get_error()", err_val));

            // Bind the catch parameter
            if let Some(param) = &handler.param {
                self.emit_binding_pattern(&param.pattern, &err_val);
            }

            for s in &handler.body.body {
                if self.block_terminated {
                    break;
                }
                self.emit_statement(s);
            }
            self.pop_scope();
            self.emit_br(after_catch);
        }

        // Finally body
        if has_finally {
            self.emit_label(&finally_label);
            self.block_terminated = false;
            if let Some(finalizer) = &stmt.finalizer {
                for s in &finalizer.body {
                    if self.block_terminated {
                        break;
                    }
                    self.emit_statement(s);
                }
            }
            self.emit_br(&end_label);
        }

        self.emit_label(&end_label);
        self.block_terminated = false;
    }

    fn emit_switch(&mut self, stmt: &SwitchStatement<'_>) {
        let disc = self.emit_expression(&stmt.discriminant);
        let end_label = self.fresh_label("sw.end");

        // Switch uses the loop stack so `break` works
        self.loop_stack.push((end_label.clone(), end_label.clone()));

        let mut next_case_label = self.fresh_label("sw.case");
        self.emit_br(&next_case_label);
        let mut fall_through_label: Option<String> = None;

        for (i, case) in stmt.cases.iter().enumerate() {
            let body_label = self.fresh_label("sw.body");
            let is_last = i == stmt.cases.len() - 1;
            let after_label = if is_last {
                end_label.clone()
            } else {
                self.fresh_label("sw.case")
            };

            if let Some(test) = &case.test {
                // case <value>:
                self.emit_label(&next_case_label);
                if let Some(ft) = &fall_through_label {
                    // If previous case fell through, merge
                    let _ = ft;
                }
                self.block_terminated = false;
                let test_val = self.emit_expression(test);
                let cmp = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i64 @js_seq(i64 {}, i64 {})",
                    cmp, disc, test_val
                ));
                let cmp_bool = self.to_bool(&cmp);
                self.emit_cond_br(&cmp_bool, &body_label, &after_label);
            } else {
                // default:
                self.emit_label(&next_case_label);
                self.block_terminated = false;
                self.emit_br(&body_label);
            }

            self.emit_label(&body_label);
            self.block_terminated = false;
            for s in &case.consequent {
                if self.block_terminated {
                    break;
                }
                self.emit_statement(s);
            }
            // Fall through to next case body (or end)
            if !self.block_terminated {
                if is_last {
                    self.emit_br(&end_label);
                } else {
                    // Fall through to next case's body
                    fall_through_label = Some(body_label.clone());
                    self.emit_br(&after_label);
                }
            }

            next_case_label = after_label;
        }

        self.loop_stack.pop();
        self.emit_label(&end_label);
        self.block_terminated = false;
    }

    fn emit_do_while(&mut self, stmt: &DoWhileStatement<'_>) {
        let body_label = self.fresh_label("dowhile.body");
        let cond_label = self.fresh_label("dowhile.cond");
        let end_label = self.fresh_label("dowhile.end");

        self.emit_br(&body_label);
        self.emit_label(&body_label);
        self.loop_stack.push((end_label.clone(), cond_label.clone()));
        self.emit_statement(&stmt.body);
        self.loop_stack.pop();
        self.emit_br(&cond_label);

        self.emit_label(&cond_label);
        let cond = self.emit_expression(&stmt.test);
        let cond_bool = self.to_bool(&cond);
        self.emit_cond_br(&cond_bool, &body_label, &end_label);

        self.emit_label(&end_label);
    }

    fn emit_for_in(&mut self, stmt: &ForInStatement<'_>) {
        self.push_scope();

        // Get keys array
        let obj = self.emit_expression(&stmt.right);
        let keys = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_object_keys_or_indices(i64 {})",
            keys, obj
        ));

        let len_key = self.emit_string_const("length");
        let len_val = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
            len_val, keys, len_key
        ));

        // Index counter
        let idx_alloca = {
            let m = format!("%forin.idx.{}", self.var_counter);
            self.var_counter += 1;
            self.emit(&format!("  {} = alloca i64, align 8", m));
            self.emit(&format!(
                "  store i64 {}, ptr {}, align 8",
                js_number_bits(0.0), m
            ));
            m
        };

        // Declare the iteration variable
        let var_name = match &stmt.left {
            ForStatementLeft::VariableDeclaration(decl) => {
                match &decl.declarations[0].id {
                    BindingPattern::BindingIdentifier(id) => id.name.as_str().to_string(),
                    _ => panic!("unsupported for-in variable pattern"),
                }
            }
            _ => panic!("unsupported for-in left-hand side"),
        };
        let iter_alloca = self.declare_var(&var_name);

        let cond_label = self.fresh_label("forin.cond");
        let body_label = self.fresh_label("forin.body");
        let update_label = self.fresh_label("forin.update");
        let end_label = self.fresh_label("forin.end");

        self.emit_br(&cond_label);
        self.emit_label(&cond_label);
        let idx = self.fresh_reg();
        self.emit(&format!("  {} = load i64, ptr {}, align 8", idx, idx_alloca));
        let cmp = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_lt(i64 {}, i64 {})",
            cmp, idx, len_val
        ));
        let cmp_bool = self.to_bool(&cmp);
        self.emit_cond_br(&cmp_bool, &body_label, &end_label);

        self.emit_label(&body_label);
        let key = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
            key, keys, idx
        ));
        self.emit(&format!(
            "  store i64 {}, ptr {}, align 8",
            key, iter_alloca
        ));

        self.loop_stack.push((end_label.clone(), update_label.clone()));
        self.emit_statement(&stmt.body);
        self.loop_stack.pop();
        self.emit_br(&update_label);

        self.emit_label(&update_label);
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

    // ---- TypeScript: enums ----

    fn emit_ts_enum(&mut self, decl: &TSEnumDeclaration<'_>) {
        let enum_name = decl.id.name.as_str();

        // Create an object for the enum
        let obj = self.fresh_reg();
        self.emit(&format!("  {} = call i64 @js_object_new()", obj));

        let mut next_value: i64 = 0;

        for member in &decl.body.members {
            let member_name = match &member.id {
                TSEnumMemberName::Identifier(id) => id.name.as_str().to_string(),
                TSEnumMemberName::String(s) => s.value.as_str().to_string(),
                _ => continue,
            };

            let val = if let Some(init) = &member.initializer {
                // Explicit initializer
                let v = self.emit_expression(init);
                // Try to track numeric value for auto-increment
                if let Expression::NumericLiteral(lit) = init {
                    next_value = lit.value as i64 + 1;
                }
                v
            } else {
                // Auto-increment numeric value
                let v = format!("{}", super::js_number_bits(next_value as f64));
                next_value += 1;
                v
            };

            // Forward mapping: Name -> Value
            let key = self.emit_string_const(&member_name);
            self.emit(&format!(
                "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                obj, key, val
            ));

            // Reverse mapping: Value -> Name (for numeric enums)
            let val_as_key = self.fresh_reg();
            self.emit(&format!(
                "  {} = call i64 @js_to_string_val(i64 {})",
                val_as_key, val
            ));
            let name_val = self.emit_string_const(&member_name);
            self.emit(&format!(
                "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                obj, val_as_key, name_val
            ));
        }

        // Store the enum object as a variable
        let alloca = self.declare_var(enum_name);
        self.emit(&format!(
            "  store i64 {}, ptr {}, align 8",
            obj, alloca
        ));

        // If in a module, also export it
        if let Some(mod_id) = self.current_module_id {
            let exports = self.fresh_reg();
            self.emit(&format!(
                "  {} = load i64, ptr @__mod_{}_exports",
                exports, mod_id
            ));
            let export_key = self.emit_string_const(enum_name);
            self.emit(&format!(
                "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                exports, export_key, obj
            ));
        }
    }

    // ---- Module: import/export ----

    fn emit_import(&mut self, decl: &ImportDeclaration<'_>) {
        let source = decl.source.value.as_str();

        // Look up module ID
        let mod_id = match self.module_map.get(source) {
            Some(&id) => id,
            None => {
                // Try with .js extension
                let with_ext = format!("{}.js", source);
                match self.module_map.get(&with_ext) {
                    Some(&id) => id,
                    None => {
                        // Try matching by filename
                        let mut found = None;
                        for (path, &id) in &self.module_map {
                            if path.ends_with(source) || path.ends_with(&format!("{}.js", source)) {
                                found = Some(id);
                                break;
                            }
                        }
                        match found {
                            Some(id) => id,
                            None => return, // Module not found, skip
                        }
                    }
                }
            }
        };

        // Call module init
        self.emit(&format!("  call void @__mod_{}_init()", mod_id));

        // Load exports object
        let exports = self.fresh_reg();
        self.emit(&format!(
            "  {} = load i64, ptr @__mod_{}_exports",
            exports, mod_id
        ));

        // Bind imported names
        if let Some(specifiers) = &decl.specifiers {
            for spec in specifiers {
                match spec {
                    ImportDeclarationSpecifier::ImportSpecifier(s) => {
                        // import { foo } from "..." or import { foo as bar } from "..."
                        let imported_name = match &s.imported {
                            ModuleExportName::IdentifierName(id) => id.name.as_str(),
                            ModuleExportName::IdentifierReference(id) => id.name.as_str(),
                            ModuleExportName::StringLiteral(sl) => sl.value.as_str(),
                        };
                        let local_name = s.local.name.as_str();
                        let key = self.emit_string_const(imported_name);
                        let val = self.fresh_reg();
                        self.emit(&format!(
                            "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
                            val, exports, key
                        ));
                        let alloca = self.declare_var(local_name);
                        self.emit(&format!(
                            "  store i64 {}, ptr {}, align 8",
                            val, alloca
                        ));
                    }
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                        // import foo from "..."
                        let local_name = s.local.name.as_str();
                        let key = self.emit_string_const("default");
                        let val = self.fresh_reg();
                        self.emit(&format!(
                            "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
                            val, exports, key
                        ));
                        let alloca = self.declare_var(local_name);
                        self.emit(&format!(
                            "  store i64 {}, ptr {}, align 8",
                            val, alloca
                        ));
                    }
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                        // import * as ns from "..."
                        let local_name = s.local.name.as_str();
                        let alloca = self.declare_var(local_name);
                        self.emit(&format!(
                            "  store i64 {}, ptr {}, align 8",
                            exports, alloca
                        ));
                    }
                }
            }
        }
    }

    fn emit_export_named(&mut self, decl: &ExportNamedDeclaration<'_>) {
        let mod_id = match self.current_module_id {
            Some(id) => id,
            None => return, // Not in a module, skip
        };

        // export const x = 5; / export function foo() {}
        if let Some(declaration) = &decl.declaration {
            match declaration {
                Declaration::VariableDeclaration(vd) => {
                    // Emit the variable declaration normally
                    self.emit_var_decl(vd);
                    // Then store each declared name in exports
                    let exports = self.fresh_reg();
                    self.emit(&format!(
                        "  {} = load i64, ptr @__mod_{}_exports",
                        exports, mod_id
                    ));
                    for declarator in &vd.declarations {
                        if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                            let name = id.name.as_str();
                            let alloca = self.lookup_var(name).to_string();
                            let val = self.fresh_reg();
                            self.emit(&format!(
                                "  {} = load i64, ptr {}, align 8",
                                val, alloca
                            ));
                            let key = self.emit_string_const(name);
                            self.emit(&format!(
                                "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                                exports, key, val
                            ));
                        }
                    }
                }
                Declaration::FunctionDeclaration(func) => {
                    self.emit_function_decl(func);
                    if let Some(id) = &func.id {
                        let name = id.name.as_str();
                        if let Some(llvm_name) = self.known_functions.get(name).cloned() {
                            let arity = func.params.items.len();
                            // Emit a trampoline: indirect convention -> direct convention
                            let tramp_name = format!("{}_tramp", llvm_name);
                            let mut tramp = String::new();
                            use std::fmt::Write;
                            writeln!(tramp, "define i64 @{}(ptr %args, i32 %argc, ptr %closure) {{", tramp_name).unwrap();
                            writeln!(tramp, "entry:").unwrap();
                            let mut param_regs = Vec::new();
                            for i in 0..arity {
                                writeln!(tramp, "  %p{} = getelementptr i64, ptr %args, i32 {}", i, i).unwrap();
                                writeln!(tramp, "  %a{} = load i64, ptr %p{}, align 8", i, i).unwrap();
                                param_regs.push(format!("i64 %a{}", i));
                            }
                            writeln!(tramp, "  %r = call i64 @{}({})", llvm_name, param_regs.join(", ")).unwrap();
                            writeln!(tramp, "  ret i64 %r").unwrap();
                            writeln!(tramp, "}}").unwrap();
                            self.functions.push(tramp);

                            let exports = self.fresh_reg();
                            self.emit(&format!(
                                "  {} = load i64, ptr @__mod_{}_exports",
                                exports, mod_id
                            ));
                            let fval = self.fresh_reg();
                            self.emit(&format!(
                                "  {} = call i64 @js_func_new(ptr @{}, ptr null, i32 {})",
                                fval, tramp_name, arity
                            ));
                            let key = self.emit_string_const(name);
                            self.emit(&format!(
                                "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                                exports, key, fval
                            ));
                        }
                    }
                }
                _ => {}
            }
        }

        // export { x, y }
        if !decl.specifiers.is_empty() && decl.source.is_none() {
            let exports = self.fresh_reg();
            self.emit(&format!(
                "  {} = load i64, ptr @__mod_{}_exports",
                exports, mod_id
            ));
            for spec in &decl.specifiers {
                let local_name = match &spec.local {
                    ModuleExportName::IdentifierName(id) => id.name.as_str(),
                    ModuleExportName::IdentifierReference(id) => id.name.as_str(),
                    ModuleExportName::StringLiteral(sl) => sl.value.as_str(),
                };
                let exported_name = match &spec.exported {
                    ModuleExportName::IdentifierName(id) => id.name.as_str(),
                    ModuleExportName::IdentifierReference(id) => id.name.as_str(),
                    ModuleExportName::StringLiteral(sl) => sl.value.as_str(),
                };
                let alloca = self.lookup_var(local_name).to_string();
                let val = self.fresh_reg();
                self.emit(&format!(
                    "  {} = load i64, ptr {}, align 8",
                    val, alloca
                ));
                let key = self.emit_string_const(exported_name);
                self.emit(&format!(
                    "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                    exports, key, val
                ));
            }
        }
    }

    fn emit_export_default(&mut self, decl: &ExportDefaultDeclaration<'_>) {
        let mod_id = match self.current_module_id {
            Some(id) => id,
            None => return,
        };

        let val = match &decl.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                // export default function foo() {}
                if func.id.is_some() {
                    self.emit_function_decl(func);
                    let name = func.id.as_ref().unwrap().name.as_str();
                    if let Some(llvm_name) = self.known_functions.get(name).cloned() {
                        let arity = func.params.items.len();
                        // Emit trampoline for indirect calling convention
                        let tramp_name = format!("{}_tramp", llvm_name);
                        let mut tramp = String::new();
                        use std::fmt::Write;
                        writeln!(tramp, "define i64 @{}(ptr %args, i32 %argc, ptr %closure) {{", tramp_name).unwrap();
                        writeln!(tramp, "entry:").unwrap();
                        let mut param_regs = Vec::new();
                        for i in 0..arity {
                            writeln!(tramp, "  %p{} = getelementptr i64, ptr %args, i32 {}", i, i).unwrap();
                            writeln!(tramp, "  %a{} = load i64, ptr %p{}, align 8", i, i).unwrap();
                            param_regs.push(format!("i64 %a{}", i));
                        }
                        writeln!(tramp, "  %r = call i64 @{}({})", llvm_name, param_regs.join(", ")).unwrap();
                        writeln!(tramp, "  ret i64 %r").unwrap();
                        writeln!(tramp, "}}").unwrap();
                        self.functions.push(tramp);

                        let fval = self.fresh_reg();
                        self.emit(&format!(
                            "  {} = call i64 @js_func_new(ptr @{}, ptr null, i32 {})",
                            fval, tramp_name, arity
                        ));
                        fval
                    } else {
                        format!("{}", JS_UNDEF)
                    }
                } else {
                    // Anonymous default export function
                    self.emit_arrow_fn(&func.params, func.body.as_ref().unwrap(), false, func.r#async)
                }
            }
            _ => {
                // export default <expression>
                if let Some(expr) = decl.declaration.as_expression() {
                    self.emit_expression(expr)
                } else {
                    format!("{}", JS_UNDEF)
                }
            }
        };

        let exports = self.fresh_reg();
        self.emit(&format!(
            "  {} = load i64, ptr @__mod_{}_exports",
            exports, mod_id
        ));
        let key = self.emit_string_const("default");
        self.emit(&format!(
            "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
            exports, key, val
        ));
    }
}
