use std::collections::HashMap;
use std::fmt::Write as FmtWrite;

use oxc_ast::ast::*;
use oxc_syntax::operator::{BinaryOperator, LogicalOperator, UnaryOperator, UpdateOperator};

// NaN-boxing constants (must match runtime.c)
const QNAN: u64 = 0x7FFC000000000000;
const SIGN_BIT: u64 = 0x8000000000000000;
const BOOL_TAG: u64 = QNAN | 0x0001000000000000;
const NULL_TAG: u64 = QNAN | 0x0002000000000000;
const UNDEF_TAG: u64 = QNAN | 0x0003000000000000;

const JS_TRUE: i64 = (BOOL_TAG | 1) as i64;
const JS_FALSE: i64 = BOOL_TAG as i64;
const JS_NULL: i64 = NULL_TAG as i64;
const JS_UNDEF: i64 = UNDEF_TAG as i64;

fn js_number_bits(v: f64) -> i64 {
    i64::from_ne_bytes(v.to_ne_bytes())
}

pub struct CodeGen {
    functions: Vec<String>,
    current_fn: String,
    next_reg: u32,
    next_label: u32,
    next_str: u32,
    next_anon_fn: u32,
    scopes: Vec<HashMap<String, String>>,
    var_counter: u32,
    string_constants: Vec<(String, String, usize)>, // (global_name, escaped, byte_len)
    known_functions: HashMap<String, String>,        // js_name -> llvm_name
    block_terminated: bool,
    current_block: String,
    is_main: bool,
}

impl CodeGen {
    fn new() -> Self {
        Self {
            functions: Vec::new(),
            current_fn: String::new(),
            next_reg: 0,
            next_label: 0,
            next_str: 0,
            next_anon_fn: 0,
            scopes: Vec::new(),
            var_counter: 0,
            string_constants: Vec::new(),
            known_functions: HashMap::new(),
            block_terminated: false,
            current_block: "entry".to_string(),
            is_main: false,
        }
    }

    pub fn compile(program: &Program<'_>) -> String {
        let mut cg = Self::new();

        // First pass: emit function declarations
        for stmt in &program.body {
            if let Statement::FunctionDeclaration(func) = stmt {
                cg.emit_function_decl(func);
            }
        }

        // Second pass: top-level code into main()
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
        self.emit(&format!("  {} = alloca i64, align 8", mangled));
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

    fn intern_string(&mut self, s: &str) -> String {
        // Check if we already have this string constant
        for (name, _, _) in &self.string_constants {
            // We'd need to compare content, but for simplicity just always create new
            // (could optimize later with dedup)
            let _ = name;
        }
        let name = format!("@.str.{}", self.next_str);
        self.next_str += 1;
        let mut escaped = String::new();
        let mut byte_len = 0usize;
        for c in s.chars() {
            match c {
                '\n' => { escaped.push_str("\\0A"); byte_len += 1; }
                '\r' => { escaped.push_str("\\0D"); byte_len += 1; }
                '\t' => { escaped.push_str("\\09"); byte_len += 1; }
                '\\' => { escaped.push_str("\\5C"); byte_len += 1; }
                '"' => { escaped.push_str("\\22"); byte_len += 1; }
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

    /// Emit a call to js_string_from_cstr for a compile-time string constant
    fn emit_string_const(&mut self, s: &str) -> String {
        let global = self.intern_string(s);
        let reg = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_string_from_cstr(ptr {})",
            reg, global
        ));
        reg
    }

    /// Convert a JSValue (i64) to a boolean (i1) for branching
    fn to_bool(&mut self, val: &str) -> String {
        let reg_i32 = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i32 @js_is_truthy(i64 {})",
            reg_i32, val
        ));
        let reg_i1 = self.fresh_reg();
        self.emit(&format!(
            "  {} = trunc i32 {} to i1",
            reg_i1, reg_i32
        ));
        reg_i1
    }

    // ---- Main function ----

    fn begin_main(&mut self) {
        self.current_fn = String::new();
        self.scopes = vec![HashMap::new()];
        self.block_terminated = false;
        self.current_block = "entry".to_string();
        self.is_main = true;
        self.emit("define i32 @main() {");
        self.emit("entry:");
        self.emit("  call void @js_runtime_init()");
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

        // Collect params
        let mut param_names = Vec::new();
        for param in &func.params.items {
            let pname = match &param.pattern {
                BindingPattern::BindingIdentifier(id) => id.name.as_str().to_string(),
                _ => panic!("unsupported parameter pattern"),
            };
            param_names.push(pname);
        }

        // Build param list
        let param_ir: Vec<String> = param_names
            .iter()
            .map(|p| format!("i64 %param.{}", p))
            .collect();

        self.emit(&format!(
            "define i64 @{}({}) {{",
            llvm_name,
            param_ir.join(", ")
        ));
        self.emit("entry:");

        // Alloca + store params
        for pname in &param_names {
            let alloca = self.declare_var(pname);
            self.emit(&format!(
                "  store i64 %param.{}, ptr {}, align 8",
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

        if !self.block_terminated {
            self.emit(&format!("  ret i64 {}", JS_UNDEF));
        }
        self.emit("}");
        self.functions.push(std::mem::take(&mut self.current_fn));

        // Restore
        self.current_fn = saved_fn;
        self.scopes = saved_scopes;
        self.block_terminated = saved_terminated;
        self.current_block = saved_block;
        self.is_main = saved_is_main;
    }

    /// Emit an arrow function / function expression as a separate LLVM function.
    /// Captures outer variables by value (shallow closure).
    /// Returns the register holding the JSValue function pointer.
    fn emit_arrow_fn(
        &mut self,
        params: &FormalParameters<'_>,
        body: &FunctionBody<'_>,
        is_expression: bool,
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

        // Collect param names
        let mut param_names = Vec::new();
        for param in &params.items {
            let pname = match &param.pattern {
                BindingPattern::BindingIdentifier(id) => id.name.as_str().to_string(),
                _ => panic!("unsupported parameter pattern"),
            };
            param_names.push(pname);
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
            if param_names.contains(name) {
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
        for (i, pname) in param_names.iter().enumerate() {
            let alloca = self.declare_var(pname);
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
            self.emit(&format!(
                "  store i64 {}, ptr {}, align 8",
                val_reg, alloca
            ));
        }

        // Emit body
        if is_expression && body.statements.len() == 1 {
            if let Some(Statement::ExpressionStatement(es)) = body.statements.first() {
                let val = self.emit_expression(&es.expression);
                if !self.block_terminated {
                    self.emit(&format!("  ret i64 {}", val));
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
            self.emit(&format!("  ret i64 {}", JS_UNDEF));
        }
        self.emit("}");
        self.functions.push(std::mem::take(&mut self.current_fn));

        // Restore state
        self.current_fn = saved_fn;
        self.scopes = saved_scopes;
        self.block_terminated = saved_terminated;
        self.current_block = saved_block;
        self.is_main = saved_is_main;

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
            result, fn_name, env_reg, param_names.len()
        ));
        result
    }

    // ---- Output ----

    fn finalize(&self) -> String {
        let mut out = String::new();
        writeln!(out, "; Generated by js-compiler").unwrap();
        writeln!(out).unwrap();

        // External declarations from runtime
        let decls = [
            "declare void @js_runtime_init()",
            "declare i64 @js_string_from_cstr(ptr)",
            "declare i64 @js_add(i64, i64)",
            "declare i64 @js_sub(i64, i64)",
            "declare i64 @js_mul(i64, i64)",
            "declare i64 @js_div(i64, i64)",
            "declare i64 @js_mod(i64, i64)",
            "declare i64 @js_neg(i64)",
            "declare i64 @js_not(i64)",
            "declare i64 @js_eq(i64, i64)",
            "declare i64 @js_neq(i64, i64)",
            "declare i64 @js_seq(i64, i64)",
            "declare i64 @js_sneq(i64, i64)",
            "declare i64 @js_lt(i64, i64)",
            "declare i64 @js_gt(i64, i64)",
            "declare i64 @js_lte(i64, i64)",
            "declare i64 @js_gte(i64, i64)",
            "declare i32 @js_is_truthy(i64)",
            "declare void @js_console_log(ptr, i32)",
            "declare void @js_console_error(ptr, i32)",
            "declare i64 @js_get_prop(i64, i64)",
            "declare void @js_set_prop(i64, i64, i64)",
            "declare i64 @js_call_method(i64, ptr, ptr, i32)",
            "declare i64 @js_object_new()",
            "declare i64 @js_array_new()",
            "declare i64 @js_array_push_val(i64, i64)",
            "declare i64 @js_typeof_val(i64)",
            "declare i64 @js_to_string_val(i64)",
            "declare i64 @js_to_number_val(i64)",
            "declare i64 @js_func_new(ptr, ptr, i32)",
            "declare i64 @js_call_func(i64, ptr, i32)",
            "declare void @js_throw(i64)",
            "declare i64 @js_prompt(i64)",
            "declare i64 @js_parse_int(i64, i64)",
            "declare i64 @js_parse_float(i64)",
            "declare i64 @js_isnan(i64)",
            "declare i64 @js_isfinite(i64)",
            "declare i64 @js_math_floor(i64)",
            "declare i64 @js_math_ceil(i64)",
            "declare i64 @js_math_round(i64)",
            "declare i64 @js_math_sqrt(i64)",
            "declare i64 @js_math_abs(i64)",
            "declare i64 @js_math_pow(i64, i64)",
            "declare i64 @js_math_log(i64)",
            "declare i64 @js_math_log2(i64)",
            "declare i64 @js_math_log10(i64)",
            "declare i64 @js_math_sin(i64)",
            "declare i64 @js_math_cos(i64)",
            "declare i64 @js_math_tan(i64)",
            "declare i64 @js_math_atan2(i64, i64)",
            "declare i64 @js_math_exp(i64)",
            "declare i64 @js_math_trunc(i64)",
            "declare i64 @js_math_sign(i64)",
            "declare i64 @js_math_random()",
            "declare i64 @js_math_min(ptr, i32)",
            "declare i64 @js_math_max(ptr, i32)",
            "declare i64 @js_Number(i64)",
            "declare i64 @js_String(i64)",
            "declare i64 @js_Boolean(i64)",
            "declare i64 @js_date_now()",
            "declare i64 @js_json_stringify(i64)",
            "declare i64 @js_object_keys(i64)",
            "declare i64 @js_object_values(i64)",
            "declare i64 @js_array_is_array(i64)",
            "declare i64 @js_error_new(ptr)",
            "declare ptr @js_alloc_closure_env(i32)",
        ];
        for d in &decls {
            writeln!(out, "{}", d).unwrap();
        }
        writeln!(out).unwrap();

        // String constants
        for (name, escaped, len) in &self.string_constants {
            writeln!(
                out,
                "{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
                name, len, escaped
            )
            .unwrap();
        }
        if !self.string_constants.is_empty() {
            writeln!(out).unwrap();
        }

        // Functions
        for func in &self.functions {
            writeln!(out, "{}", func).unwrap();
        }

        out
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

    fn emit_try(&mut self, _stmt: &TryStatement<'_>) {
        // Basic try/catch is complex with setjmp — for now, just execute the try body
        // and skip catch. Errors will still exit via js_throw.
        if let Some(block) = &_stmt.block.body.first() {
            // Just emit the try block body directly
            let _ = block;
        }
        for stmt in &_stmt.block.body {
            self.emit_statement(stmt);
        }
        // TODO: implement proper try/catch with setjmp/longjmp
    }

    // ---- Expression emission ----

    fn emit_expression(&mut self, expr: &Expression<'_>) -> String {
        match expr {
            Expression::NumericLiteral(lit) => format!("{}", js_number_bits(lit.value)),
            Expression::BooleanLiteral(lit) => {
                format!("{}", if lit.value { JS_TRUE } else { JS_FALSE })
            }
            Expression::NullLiteral(_) => format!("{}", JS_NULL),
            Expression::StringLiteral(lit) => self.emit_string_const(lit.value.as_str()),
            Expression::TemplateLiteral(tl) => self.emit_template_literal(tl),
            Expression::Identifier(id) => {
                match id.name.as_str() {
                    "undefined" => return format!("{}", JS_UNDEF),
                    "NaN" => return format!("{}", js_number_bits(f64::NAN)),
                    "Infinity" => return format!("{}", js_number_bits(f64::INFINITY)),
                    _ => {}
                }
                let alloca = self.lookup_var(id.name.as_str()).to_string();
                let reg = self.fresh_reg();
                self.emit(&format!("  {} = load i64, ptr {}, align 8", reg, alloca));
                reg
            }
            Expression::BinaryExpression(be) => self.emit_binary(be),
            Expression::UnaryExpression(ue) => self.emit_unary(ue),
            Expression::LogicalExpression(le) => self.emit_logical(le),
            Expression::AssignmentExpression(ae) => self.emit_assignment(ae),
            Expression::CallExpression(ce) => self.emit_call(ce),
            Expression::UpdateExpression(ue) => self.emit_update(ue),
            Expression::ParenthesizedExpression(pe) => self.emit_expression(&pe.expression),
            Expression::ConditionalExpression(ce) => self.emit_conditional(ce),
            Expression::ObjectExpression(oe) => self.emit_object(oe),
            Expression::ArrayExpression(ae) => self.emit_array(ae),
            Expression::StaticMemberExpression(sme) => self.emit_static_member(sme),
            Expression::ComputedMemberExpression(cme) => self.emit_computed_member(cme),
            Expression::ArrowFunctionExpression(af) => {
                self.emit_arrow_fn(&af.params, af.body.as_ref(), af.expression)
            }
            Expression::FunctionExpression(fe) => {
                if let Some(body) = &fe.body {
                    self.emit_arrow_fn(&fe.params, body, false)
                } else {
                    format!("{}", JS_UNDEF)
                }
            }
            _ => format!("{}", JS_UNDEF), // unsupported → undefined
        }
    }

    fn emit_binary(&mut self, expr: &BinaryExpression<'_>) -> String {
        let left = self.emit_expression(&expr.left);
        let right = self.emit_expression(&expr.right);
        let reg = self.fresh_reg();

        let func = match expr.operator {
            BinaryOperator::Addition => "js_add",
            BinaryOperator::Subtraction => "js_sub",
            BinaryOperator::Multiplication => "js_mul",
            BinaryOperator::Division => "js_div",
            BinaryOperator::Remainder => "js_mod",
            BinaryOperator::LessThan => "js_lt",
            BinaryOperator::GreaterThan => "js_gt",
            BinaryOperator::LessEqualThan => "js_lte",
            BinaryOperator::GreaterEqualThan => "js_gte",
            BinaryOperator::Equality => "js_eq",
            BinaryOperator::Inequality => "js_neq",
            BinaryOperator::StrictEquality => "js_seq",
            BinaryOperator::StrictInequality => "js_sneq",
            _ => panic!("unsupported binary operator: {:?}", expr.operator),
        };

        self.emit(&format!(
            "  {} = call i64 @{}(i64 {}, i64 {})",
            reg, func, left, right
        ));
        reg
    }

    fn emit_unary(&mut self, expr: &UnaryExpression<'_>) -> String {
        if expr.operator == UnaryOperator::Typeof {
            // typeof — special handling for identifiers (returns "undefined" instead of throwing)
            let val = self.emit_expression(&expr.argument);
            let reg = self.fresh_reg();
            self.emit(&format!(
                "  {} = call i64 @js_typeof_val(i64 {})",
                reg, val
            ));
            return reg;
        }
        let operand = self.emit_expression(&expr.argument);
        let reg = self.fresh_reg();
        match expr.operator {
            UnaryOperator::UnaryNegation => {
                self.emit(&format!("  {} = call i64 @js_neg(i64 {})", reg, operand));
            }
            UnaryOperator::LogicalNot => {
                self.emit(&format!("  {} = call i64 @js_not(i64 {})", reg, operand));
            }
            UnaryOperator::UnaryPlus => {
                self.emit(&format!(
                    "  {} = call i64 @js_to_number_val(i64 {})",
                    reg, operand
                ));
            }
            _ => {
                return format!("{}", JS_UNDEF);
            }
        }
        reg
    }

    fn emit_logical(&mut self, expr: &LogicalExpression<'_>) -> String {
        let left = self.emit_expression(&expr.left);
        let left_bool = self.to_bool(&left);
        let left_block = self.current_block.clone();

        let rhs_label = self.fresh_label("logic.rhs");
        let end_label = self.fresh_label("logic.end");

        match expr.operator {
            LogicalOperator::And => {
                self.emit_cond_br(&left_bool, &rhs_label, &end_label);
            }
            LogicalOperator::Or => {
                self.emit_cond_br(&left_bool, &end_label, &rhs_label);
            }
            _ => {
                self.emit_cond_br(&left_bool, &end_label, &rhs_label);
            }
        }

        self.emit_label(&rhs_label);
        let right = self.emit_expression(&expr.right);
        let rhs_block = self.current_block.clone();
        self.emit_br(&end_label);

        self.emit_label(&end_label);
        let result = self.fresh_reg();
        self.emit(&format!(
            "  {} = phi i64 [ {}, %{} ], [ {}, %{} ]",
            result, left, left_block, right, rhs_block
        ));
        result
    }

    fn emit_assignment(&mut self, expr: &AssignmentExpression<'_>) -> String {
        let val = self.emit_expression(&expr.right);
        match &expr.left {
            AssignmentTarget::AssignmentTargetIdentifier(id) => {
                let alloca = self.lookup_var(id.name.as_str()).to_string();
                self.emit(&format!("  store i64 {}, ptr {}, align 8", val, alloca));
            }
            AssignmentTarget::StaticMemberExpression(sme) => {
                let obj = self.emit_expression(&sme.object);
                let key = self.emit_string_const(sme.property.name.as_str());
                self.emit(&format!(
                    "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                    obj, key, val
                ));
            }
            AssignmentTarget::ComputedMemberExpression(cme) => {
                let obj = self.emit_expression(&cme.object);
                let key = self.emit_expression(&cme.expression);
                self.emit(&format!(
                    "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                    obj, key, val
                ));
            }
            _ => panic!("unsupported assignment target"),
        }
        val
    }

    fn emit_call(&mut self, expr: &CallExpression<'_>) -> String {
        // Detect special calls
        if let Some(result) = self.try_emit_builtin_call(expr) {
            return result;
        }

        // Method call: obj.method(args)
        if let Expression::StaticMemberExpression(sme) = &expr.callee {
            return self.emit_method_call(sme, &expr.arguments);
        }

        // Direct call to known function
        if let Expression::Identifier(id) = &expr.callee {
            let name = id.name.as_str().to_string();
            if let Some(llvm_name) = self.known_functions.get(&name).cloned() {
                // Direct call with i64 params
                let mut arg_regs = Vec::new();
                for arg in &expr.arguments {
                    arg_regs.push(self.emit_call_arg(arg));
                }
                let params: Vec<String> =
                    arg_regs.iter().map(|r| format!("i64 {}", r)).collect();
                let result = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i64 @{}({})",
                    result,
                    llvm_name,
                    params.join(", ")
                ));
                return result;
            }

            // Indirect call through variable (function value)
            let func_val = {
                let alloca = self.lookup_var(id.name.as_str()).to_string();
                let r = self.fresh_reg();
                self.emit(&format!("  {} = load i64, ptr {}, align 8", r, alloca));
                r
            };
            return self.emit_indirect_call(&func_val, &expr.arguments);
        }

        // Indirect call (e.g., arr[0](), someExpr())
        let func_val = self.emit_expression(&expr.callee);
        self.emit_indirect_call(&func_val, &expr.arguments)
    }

    fn emit_indirect_call(&mut self, func_val: &str, arguments: &[Argument<'_>]) -> String {
        let argc = arguments.len();
        let args_alloca = self.fresh_reg();
        self.emit(&format!(
            "  {} = alloca i64, i32 {}",
            args_alloca,
            if argc == 0 { 1 } else { argc }
        ));
        for (i, arg) in arguments.iter().enumerate() {
            let val = self.emit_call_arg(arg);
            let ptr = self.fresh_reg();
            self.emit(&format!(
                "  {} = getelementptr i64, ptr {}, i32 {}",
                ptr, args_alloca, i
            ));
            self.emit(&format!("  store i64 {}, ptr {}, align 8", val, ptr));
        }
        let result = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_call_func(i64 {}, ptr {}, i32 {})",
            result, func_val, args_alloca, argc
        ));
        result
    }

    /// Try to emit a built-in function call. Returns Some(reg) if handled.
    fn try_emit_builtin_call(&mut self, expr: &CallExpression<'_>) -> Option<String> {
        // console.log / console.error
        if let Expression::StaticMemberExpression(sme) = &expr.callee {
            if let Expression::Identifier(obj) = &sme.object {
                let obj_name = obj.name.as_str();
                let method = sme.property.name.as_str();

                if obj_name == "console" && (method == "log" || method == "error") {
                    return Some(self.emit_console_call(method, &expr.arguments));
                }

                // Math methods
                if obj_name == "Math" {
                    return self.emit_math_call(method, &expr.arguments);
                }

                // Date.now()
                if obj_name == "Date" && method == "now" {
                    let r = self.fresh_reg();
                    self.emit(&format!("  {} = call i64 @js_date_now()", r));
                    return Some(r);
                }

                // JSON.stringify
                if obj_name == "JSON" && method == "stringify" {
                    let arg = if expr.arguments.is_empty() {
                        format!("{}", JS_UNDEF)
                    } else {
                        self.emit_call_arg(&expr.arguments[0])
                    };
                    let r = self.fresh_reg();
                    self.emit(&format!(
                        "  {} = call i64 @js_json_stringify(i64 {})",
                        r, arg
                    ));
                    return Some(r);
                }

                // Object.keys / Object.values
                if obj_name == "Object" && method == "keys" {
                    let arg = if expr.arguments.is_empty() {
                        format!("{}", JS_UNDEF)
                    } else {
                        self.emit_call_arg(&expr.arguments[0])
                    };
                    let r = self.fresh_reg();
                    self.emit(&format!(
                        "  {} = call i64 @js_object_keys(i64 {})",
                        r, arg
                    ));
                    return Some(r);
                }
                if obj_name == "Object" && method == "values" {
                    let arg = if expr.arguments.is_empty() {
                        format!("{}", JS_UNDEF)
                    } else {
                        self.emit_call_arg(&expr.arguments[0])
                    };
                    let r = self.fresh_reg();
                    self.emit(&format!(
                        "  {} = call i64 @js_object_values(i64 {})",
                        r, arg
                    ));
                    return Some(r);
                }

                // Array.isArray
                if obj_name == "Array" && method == "isArray" {
                    let arg = if expr.arguments.is_empty() {
                        format!("{}", JS_UNDEF)
                    } else {
                        self.emit_call_arg(&expr.arguments[0])
                    };
                    let r = self.fresh_reg();
                    self.emit(&format!(
                        "  {} = call i64 @js_array_is_array(i64 {})",
                        r, arg
                    ));
                    return Some(r);
                }
            }
        }

        // Global builtins: prompt, parseInt, parseFloat, isNaN, isFinite, Number, String, Boolean
        if let Expression::Identifier(id) = &expr.callee {
            let name = id.name.as_str();
            match name {
                "prompt" => {
                    let arg = if expr.arguments.is_empty() {
                        format!("{}", JS_UNDEF)
                    } else {
                        self.emit_call_arg(&expr.arguments[0])
                    };
                    let r = self.fresh_reg();
                    self.emit(&format!("  {} = call i64 @js_prompt(i64 {})", r, arg));
                    return Some(r);
                }
                "parseInt" => {
                    let arg0 = if expr.arguments.is_empty() {
                        format!("{}", JS_UNDEF)
                    } else {
                        self.emit_call_arg(&expr.arguments[0])
                    };
                    let arg1 = if expr.arguments.len() >= 2 {
                        self.emit_call_arg(&expr.arguments[1])
                    } else {
                        format!("{}", JS_UNDEF)
                    };
                    let r = self.fresh_reg();
                    self.emit(&format!(
                        "  {} = call i64 @js_parse_int(i64 {}, i64 {})",
                        r, arg0, arg1
                    ));
                    return Some(r);
                }
                "parseFloat" => {
                    let arg = if expr.arguments.is_empty() {
                        format!("{}", JS_UNDEF)
                    } else {
                        self.emit_call_arg(&expr.arguments[0])
                    };
                    let r = self.fresh_reg();
                    self.emit(&format!(
                        "  {} = call i64 @js_parse_float(i64 {})",
                        r, arg
                    ));
                    return Some(r);
                }
                "isNaN" => {
                    let arg = self.emit_call_arg(&expr.arguments[0]);
                    let r = self.fresh_reg();
                    self.emit(&format!("  {} = call i64 @js_isnan(i64 {})", r, arg));
                    return Some(r);
                }
                "isFinite" => {
                    let arg = self.emit_call_arg(&expr.arguments[0]);
                    let r = self.fresh_reg();
                    self.emit(&format!("  {} = call i64 @js_isfinite(i64 {})", r, arg));
                    return Some(r);
                }
                "Number" => {
                    let arg = self.emit_call_arg(&expr.arguments[0]);
                    let r = self.fresh_reg();
                    self.emit(&format!("  {} = call i64 @js_Number(i64 {})", r, arg));
                    return Some(r);
                }
                "String" => {
                    let arg = self.emit_call_arg(&expr.arguments[0]);
                    let r = self.fresh_reg();
                    self.emit(&format!("  {} = call i64 @js_String(i64 {})", r, arg));
                    return Some(r);
                }
                "Boolean" => {
                    let arg = self.emit_call_arg(&expr.arguments[0]);
                    let r = self.fresh_reg();
                    self.emit(&format!("  {} = call i64 @js_Boolean(i64 {})", r, arg));
                    return Some(r);
                }
                _ => {}
            }
        }

        None
    }

    fn emit_console_call(&mut self, method: &str, arguments: &[Argument<'_>]) -> String {
        let argc = arguments.len();
        let args_alloca = self.fresh_reg();
        self.emit(&format!(
            "  {} = alloca i64, i32 {}",
            args_alloca,
            if argc == 0 { 1 } else { argc }
        ));
        for (i, arg) in arguments.iter().enumerate() {
            let val = self.emit_call_arg(arg);
            let ptr = self.fresh_reg();
            self.emit(&format!(
                "  {} = getelementptr i64, ptr {}, i32 {}",
                ptr, args_alloca, i
            ));
            self.emit(&format!("  store i64 {}, ptr {}, align 8", val, ptr));
        }
        let func = if method == "error" {
            "js_console_error"
        } else {
            "js_console_log"
        };
        self.emit(&format!(
            "  call void @{}(ptr {}, i32 {})",
            func, args_alloca, argc
        ));
        format!("{}", JS_UNDEF)
    }

    fn emit_math_call(&mut self, method: &str, arguments: &[Argument<'_>]) -> Option<String> {
        let r = self.fresh_reg();
        match method {
            "floor" | "ceil" | "round" | "sqrt" | "abs" | "log" | "log2" | "log10" | "sin"
            | "cos" | "tan" | "exp" | "trunc" | "sign" => {
                let arg = self.emit_call_arg(&arguments[0]);
                self.emit(&format!(
                    "  {} = call i64 @js_math_{}(i64 {})",
                    r, method, arg
                ));
                Some(r)
            }
            "pow" | "atan2" => {
                let a = self.emit_call_arg(&arguments[0]);
                let b = self.emit_call_arg(&arguments[1]);
                self.emit(&format!(
                    "  {} = call i64 @js_math_{}(i64 {}, i64 {})",
                    r, method, a, b
                ));
                Some(r)
            }
            "random" => {
                self.emit(&format!("  {} = call i64 @js_math_random()", r));
                Some(r)
            }
            "min" | "max" => {
                let argc = arguments.len();
                let alloca = self.fresh_reg();
                self.emit(&format!(
                    "  {} = alloca i64, i32 {}",
                    alloca,
                    if argc == 0 { 1 } else { argc }
                ));
                for (i, arg) in arguments.iter().enumerate() {
                    let val = self.emit_call_arg(arg);
                    let ptr = self.fresh_reg();
                    self.emit(&format!(
                        "  {} = getelementptr i64, ptr {}, i32 {}",
                        ptr, alloca, i
                    ));
                    self.emit(&format!("  store i64 {}, ptr {}, align 8", val, ptr));
                }
                self.emit(&format!(
                    "  {} = call i64 @js_math_{}(ptr {}, i32 {})",
                    r, method, alloca, argc
                ));
                Some(r)
            }
            "PI" | "E" | "LN2" | "LN10" | "SQRT2" => {
                // These are properties, not methods — handled in static member access
                None
            }
            _ => None,
        }
    }

    fn emit_method_call(
        &mut self,
        sme: &StaticMemberExpression<'_>,
        arguments: &[Argument<'_>],
    ) -> String {
        let obj = self.emit_expression(&sme.object);
        let method_name = sme.property.name.as_str();
        let method_global = self.intern_string(method_name);

        let argc = arguments.len();
        let args_alloca = self.fresh_reg();
        self.emit(&format!(
            "  {} = alloca i64, i32 {}",
            args_alloca,
            if argc == 0 { 1 } else { argc }
        ));
        for (i, arg) in arguments.iter().enumerate() {
            let val = self.emit_call_arg(arg);
            let ptr = self.fresh_reg();
            self.emit(&format!(
                "  {} = getelementptr i64, ptr {}, i32 {}",
                ptr, args_alloca, i
            ));
            self.emit(&format!("  store i64 {}, ptr {}, align 8", val, ptr));
        }

        let result = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_call_method(i64 {}, ptr {}, ptr {}, i32 {})",
            result, obj, method_global, args_alloca, argc
        ));
        result
    }

    fn emit_conditional(&mut self, expr: &ConditionalExpression<'_>) -> String {
        let cond = self.emit_expression(&expr.test);
        let cond_bool = self.to_bool(&cond);
        let cond_block = self.current_block.clone();
        let _ = cond_block;

        let then_label = self.fresh_label("tern.then");
        let else_label = self.fresh_label("tern.else");
        let end_label = self.fresh_label("tern.end");

        self.emit_cond_br(&cond_bool, &then_label, &else_label);

        self.emit_label(&then_label);
        let then_val = self.emit_expression(&expr.consequent);
        let then_block = self.current_block.clone();
        self.emit_br(&end_label);

        self.emit_label(&else_label);
        let else_val = self.emit_expression(&expr.alternate);
        let else_block = self.current_block.clone();
        self.emit_br(&end_label);

        self.emit_label(&end_label);
        let result = self.fresh_reg();
        self.emit(&format!(
            "  {} = phi i64 [ {}, %{} ], [ {}, %{} ]",
            result, then_val, then_block, else_val, else_block
        ));
        result
    }

    fn emit_object(&mut self, expr: &ObjectExpression<'_>) -> String {
        let obj = self.fresh_reg();
        self.emit(&format!("  {} = call i64 @js_object_new()", obj));

        for prop in &expr.properties {
            match prop {
                ObjectPropertyKind::ObjectProperty(p) => {
                    let key = match &p.key {
                        PropertyKey::StaticIdentifier(id) => {
                            self.emit_string_const(id.name.as_str())
                        }
                        PropertyKey::StringLiteral(s) => {
                            self.emit_string_const(s.value.as_str())
                        }
                        PropertyKey::NumericLiteral(n) => {
                            let s = format!("{}", n.value);
                            self.emit_string_const(&s)
                        }
                        _ => self.emit_string_const("unknown"),
                    };
                    let val = self.emit_expression(&p.value);
                    self.emit(&format!(
                        "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                        obj, key, val
                    ));
                }
                _ => {} // skip spread, etc.
            }
        }
        obj
    }

    fn emit_array(&mut self, expr: &ArrayExpression<'_>) -> String {
        let arr = self.fresh_reg();
        self.emit(&format!("  {} = call i64 @js_array_new()", arr));

        for elem in &expr.elements {
            match elem {
                ArrayExpressionElement::SpreadElement(_) => {
                    // TODO: spread
                }
                _ => {
                    if let Some(expr) = elem.as_expression() {
                        let val = self.emit_expression(expr);
                        self.emit(&format!(
                            "  call i64 @js_array_push_val(i64 {}, i64 {})",
                            arr, val
                        ));
                    }
                }
            }
        }
        arr
    }

    fn emit_static_member(&mut self, sme: &StaticMemberExpression<'_>) -> String {
        // Math constants
        if let Expression::Identifier(obj) = &sme.object {
            if obj.name.as_str() == "Math" {
                let val = match sme.property.name.as_str() {
                    "PI" => std::f64::consts::PI,
                    "E" => std::f64::consts::E,
                    "LN2" => std::f64::consts::LN_2,
                    "LN10" => std::f64::consts::LN_10,
                    "SQRT2" => std::f64::consts::SQRT_2,
                    "LOG2E" => std::f64::consts::LOG2_E,
                    "LOG10E" => std::f64::consts::LOG10_E,
                    _ => {
                        // Might be a method used as property — return undefined
                        return format!("{}", JS_UNDEF);
                    }
                };
                return format!("{}", js_number_bits(val));
            }
        }

        // General property access: obj.prop → js_get_prop(obj, "prop")
        let obj = self.emit_expression(&sme.object);
        let key = self.emit_string_const(sme.property.name.as_str());
        let result = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
            result, obj, key
        ));
        result
    }

    fn emit_computed_member(&mut self, cme: &ComputedMemberExpression<'_>) -> String {
        let obj = self.emit_expression(&cme.object);
        let key = self.emit_expression(&cme.expression);
        let result = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
            result, obj, key
        ));
        result
    }

    fn emit_template_literal(&mut self, tl: &TemplateLiteral<'_>) -> String {
        // Start with the first quasi
        let first_quasi = &tl.quasis[0];
        let mut result = self.emit_string_const(
            first_quasi.value.raw.as_str(),
        );

        // Alternate: expression, quasi, expression, quasi, ...
        for (i, expr) in tl.expressions.iter().enumerate() {
            let val = self.emit_expression(expr);
            let val_str = self.fresh_reg();
            self.emit(&format!(
                "  {} = call i64 @js_to_string_val(i64 {})",
                val_str, val
            ));
            let concat1 = self.fresh_reg();
            self.emit(&format!(
                "  {} = call i64 @js_add(i64 {}, i64 {})",
                concat1, result, val_str
            ));

            let quasi = &tl.quasis[i + 1];
            let quasi_str = self.emit_string_const(quasi.value.raw.as_str());
            let concat2 = self.fresh_reg();
            self.emit(&format!(
                "  {} = call i64 @js_add(i64 {}, i64 {})",
                concat2, concat1, quasi_str
            ));
            result = concat2;
        }
        result
    }

    fn emit_update(&mut self, expr: &UpdateExpression<'_>) -> String {
        let var_name = match &expr.argument {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                id.name.as_str().to_string()
            }
            _ => panic!("unsupported update target"),
        };

        let alloca = self.lookup_var(&var_name).to_string();
        let old_val = self.fresh_reg();
        self.emit(&format!(
            "  {} = load i64, ptr {}, align 8",
            old_val, alloca
        ));

        let one = js_number_bits(1.0);
        let new_val = self.fresh_reg();
        let func = match expr.operator {
            UpdateOperator::Increment => "js_add",
            UpdateOperator::Decrement => "js_sub",
        };
        self.emit(&format!(
            "  {} = call i64 @{}(i64 {}, i64 {})",
            new_val, func, old_val, one
        ));

        self.emit(&format!(
            "  store i64 {}, ptr {}, align 8",
            new_val, alloca
        ));

        if expr.prefix {
            new_val
        } else {
            old_val
        }
    }

    fn emit_call_arg(&mut self, arg: &Argument<'_>) -> String {
        match arg {
            Argument::NumericLiteral(lit) => format!("{}", js_number_bits(lit.value)),
            Argument::BooleanLiteral(lit) => {
                format!("{}", if lit.value { JS_TRUE } else { JS_FALSE })
            }
            Argument::NullLiteral(_) => format!("{}", JS_NULL),
            Argument::StringLiteral(lit) => self.emit_string_const(lit.value.as_str()),
            Argument::TemplateLiteral(tl) => self.emit_template_literal(tl),
            Argument::Identifier(id) => {
                match id.name.as_str() {
                    "undefined" => return format!("{}", JS_UNDEF),
                    "NaN" => return format!("{}", js_number_bits(f64::NAN)),
                    "Infinity" => return format!("{}", js_number_bits(f64::INFINITY)),
                    _ => {}
                }
                let alloca = self.lookup_var(id.name.as_str()).to_string();
                let reg = self.fresh_reg();
                self.emit(&format!("  {} = load i64, ptr {}, align 8", reg, alloca));
                reg
            }
            Argument::BinaryExpression(be) => self.emit_binary(be),
            Argument::UnaryExpression(ue) => self.emit_unary(ue),
            Argument::LogicalExpression(le) => self.emit_logical(le),
            Argument::CallExpression(ce) => self.emit_call(ce),
            Argument::ParenthesizedExpression(pe) => self.emit_expression(&pe.expression),
            Argument::AssignmentExpression(ae) => self.emit_assignment(ae),
            Argument::UpdateExpression(ue) => self.emit_update(ue),
            Argument::ConditionalExpression(ce) => self.emit_conditional(ce),
            Argument::ObjectExpression(oe) => self.emit_object(oe),
            Argument::ArrayExpression(ae) => self.emit_array(ae),
            Argument::StaticMemberExpression(sme) => self.emit_static_member(sme),
            Argument::ComputedMemberExpression(cme) => self.emit_computed_member(cme),
            Argument::ArrowFunctionExpression(af) => {
                self.emit_arrow_fn(&af.params, af.body.as_ref(), af.expression)
            }
            Argument::FunctionExpression(fe) => {
                if let Some(body) = &fe.body {
                    self.emit_arrow_fn(&fe.params, body, false)
                } else {
                    format!("{}", JS_UNDEF)
                }
            }
            _ => format!("{}", JS_UNDEF),
        }
    }
}
