use std::collections::HashMap;

use oxc_ast::ast::*;

use super::{CodeGen, JS_UNDEF};

impl CodeGen {
    pub(crate) fn emit_function_decl(&mut self, func: &Function<'_>) {
        let js_name = func.id.as_ref().expect("function must have name").name.as_str();
        let llvm_name = format!("__jsfn_{}", js_name);
        self.known_functions
            .insert(js_name.to_string(), llvm_name.clone());

        // Save state
        let saved_fn = std::mem::take(&mut self.current_fn);
        let saved_scopes = std::mem::take(&mut self.scopes);
        let saved_terminated = self.block_terminated;
        let saved_block = std::mem::take(&mut self.current_block);
        let saved_is_main = self.is_main;

        self.scopes = vec![HashMap::new()];
        self.block_terminated = false;
        self.current_block = "entry".to_string();
        self.is_main = false;
        self.is_async = func.r#async;

        // Collect params — use simple names for identifiers, synthetic names for destructured
        let mut param_ir_names = Vec::new();
        for (i, param) in func.params.items.iter().enumerate() {
            let ir_name = match &param.pattern {
                BindingPattern::BindingIdentifier(id) => id.name.as_str().to_string(),
                _ => format!("__arg{}", i),
            };
            param_ir_names.push(ir_name);
        }

        // Build param list
        let param_ir: Vec<String> = param_ir_names
            .iter()
            .map(|p| format!("i64 %param.{}", p))
            .collect();

        self.emit(&format!(
            "define i64 @{}({}) {{",
            llvm_name,
            param_ir.join(", ")
        ));
        self.emit("entry:");

        // Bind params — simple names get alloca+store, destructured get emit_binding_pattern
        for (i, param) in func.params.items.iter().enumerate() {
            let ir_name = &param_ir_names[i];
            match &param.pattern {
                BindingPattern::BindingIdentifier(id) => {
                    let alloca = self.declare_var(id.name.as_str());
                    self.emit(&format!(
                        "  store i64 %param.{}, ptr {}, align 8",
                        ir_name, alloca
                    ));
                }
                pattern => {
                    // Store the param into a temp alloca, then destructure
                    let tmp = format!("%param.tmp.{}", self.var_counter);
                    self.var_counter += 1;
                    self.emit(&format!("  {} = alloca i64, align 8", tmp));
                    self.emit(&format!(
                        "  store i64 %param.{}, ptr {}, align 8",
                        ir_name, tmp
                    ));
                    let val = self.fresh_reg();
                    self.emit(&format!("  {} = load i64, ptr {}, align 8", val, tmp));
                    self.emit_binding_pattern(pattern, &val);
                }
            }
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

        if !self.block_terminated {
            if self.is_async {
                let wrapped = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i64 @js_async_return(i64 {})",
                    wrapped, JS_UNDEF
                ));
                self.emit(&format!("  ret i64 {}", wrapped));
            } else {
                self.emit(&format!("  ret i64 {}", JS_UNDEF));
            }
        }
        self.emit("}");
        self.functions.push(std::mem::take(&mut self.current_fn));

        // Restore
        self.current_fn = saved_fn;
        self.scopes = saved_scopes;
        self.block_terminated = saved_terminated;
        self.current_block = saved_block;
        self.is_main = saved_is_main;
        self.is_async = false;
    }

    /// Emit an arrow function / function expression as a separate LLVM function.
    /// Captures outer variables by value (shallow closure).
    /// Returns the register holding the JSValue function pointer.
    pub(crate) fn emit_arrow_fn(
        &mut self,
        params: &FormalParameters<'_>,
        body: &FunctionBody<'_>,
        is_expression: bool,
        is_async_fn: bool,
    ) -> String {
        let fn_name = format!("__jsfn_anon_{}", self.next_anon_fn);
        self.next_anon_fn += 1;

        // Collect all outer variables (for closure capture)
        let outer_vars: Vec<(String, String)> = self
            .scopes
            .iter()
            .flat_map(|scope| scope.iter().map(|(k, v)| (k.clone(), v.clone())))
            .collect();

        // Save state
        let saved_fn = std::mem::take(&mut self.current_fn);
        let saved_scopes = std::mem::take(&mut self.scopes);
        let saved_terminated = self.block_terminated;
        let saved_block = std::mem::take(&mut self.current_block);
        let saved_is_main = self.is_main;

        self.scopes = vec![HashMap::new()];
        self.block_terminated = false;
        self.current_block = "entry".to_string();
        self.is_main = false;
        self.is_async = is_async_fn;

        // Collect simple param names (for closure shadowing check)
        let mut simple_param_names: Vec<String> = Vec::new();
        for param in &params.items {
            if let BindingPattern::BindingIdentifier(id) = &param.pattern {
                simple_param_names.push(id.name.as_str().to_string());
            }
        }

        // Arrow/function expressions use indirect calling convention:
        // i64 fn(ptr %args, i32 %argc, ptr %closure)
        self.emit(&format!(
            "define i64 @{}(ptr %args, i32 %argc, ptr %closure) {{",
            fn_name
        ));
        self.emit("entry:");

        // Restore captured variables from closure env into local allocas
        for (i, (name, _)) in outer_vars.iter().enumerate() {
            // Skip if a parameter has the same name (param takes precedence)
            if simple_param_names.contains(name) {
                continue;
            }
            let alloca = self.declare_var(name);
            let ptr_reg = self.fresh_reg();
            self.emit(&format!(
                "  {} = getelementptr i64, ptr %closure, i32 {}",
                ptr_reg, i
            ));
            let val_reg = self.fresh_reg();
            self.emit(&format!(
                "  {} = load i64, ptr {}, align 8",
                val_reg, ptr_reg
            ));
            self.emit(&format!(
                "  store i64 {}, ptr {}, align 8",
                val_reg, alloca
            ));
        }

        // Unpack args into local variables (after captures, so params override)
        for (i, param) in params.items.iter().enumerate() {
            let ptr_reg = self.fresh_reg();
            self.emit(&format!(
                "  {} = getelementptr i64, ptr %args, i32 {}",
                ptr_reg, i
            ));
            let val_reg = self.fresh_reg();
            self.emit(&format!(
                "  {} = load i64, ptr {}, align 8",
                val_reg, ptr_reg
            ));
            match &param.pattern {
                BindingPattern::BindingIdentifier(id) => {
                    let alloca = self.declare_var(id.name.as_str());
                    self.emit(&format!(
                        "  store i64 {}, ptr {}, align 8",
                        val_reg, alloca
                    ));
                }
                pattern => {
                    self.emit_binding_pattern(pattern, &val_reg);
                }
            }
        }

        // Emit body
        if is_expression && body.statements.len() == 1 {
            if let Some(Statement::ExpressionStatement(es)) = body.statements.first() {
                let val = self.emit_expression(&es.expression);
                if !self.block_terminated {
                    if is_async_fn {
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
            }
        } else {
            for stmt in &body.statements {
                if self.block_terminated {
                    break;
                }
                self.emit_statement(stmt);
            }
        }

        if !self.block_terminated {
            if is_async_fn {
                let wrapped = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i64 @js_async_return(i64 {})",
                    wrapped, JS_UNDEF
                ));
                self.emit(&format!("  ret i64 {}", wrapped));
            } else {
                self.emit(&format!("  ret i64 {}", JS_UNDEF));
            }
        }
        self.emit("}");
        self.functions.push(std::mem::take(&mut self.current_fn));

        // Restore state
        self.current_fn = saved_fn;
        self.scopes = saved_scopes;
        self.block_terminated = saved_terminated;
        self.current_block = saved_block;
        self.is_main = saved_is_main;
        self.is_async = false;

        // Allocate and populate closure environment in the OUTER function
        let env_size = outer_vars.len();
        let env_reg = if env_size > 0 {
            let env = self.fresh_reg();
            self.emit(&format!(
                "  {} = call ptr @js_alloc_closure_env(i32 {})",
                env, env_size
            ));
            // Store each outer variable's current value into the env
            for (i, (_, alloca)) in outer_vars.iter().enumerate() {
                let val = self.fresh_reg();
                self.emit(&format!(
                    "  {} = load i64, ptr {}, align 8",
                    val, alloca
                ));
                let ptr = self.fresh_reg();
                self.emit(&format!(
                    "  {} = getelementptr i64, ptr {}, i32 {}",
                    ptr, env, i
                ));
                self.emit(&format!("  store i64 {}, ptr {}, align 8", val, ptr));
            }
            env
        } else {
            "null".to_string()
        };

        // Create function value with closure env
        let result = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_func_new(ptr @{}, ptr {}, i32 {})",
            result, fn_name, env_reg, params.items.len()
        ));
        result
    }
}
