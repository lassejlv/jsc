mod calls;
mod expressions;
mod functions;
mod literals;
mod statements;

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;

use oxc_ast::ast::*;

// NaN-boxing constants (must match runtime.c)
pub(crate) const QNAN: u64 = 0x7FFC000000000000;
pub(crate) const SIGN_BIT: u64 = 0x8000000000000000;
pub(crate) const BOOL_TAG: u64 = QNAN | 0x0001000000000000;
pub(crate) const NULL_TAG: u64 = QNAN | 0x0002000000000000;
pub(crate) const UNDEF_TAG: u64 = QNAN | 0x0003000000000000;

pub(crate) const JS_TRUE: i64 = (BOOL_TAG | 1) as i64;
pub(crate) const JS_FALSE: i64 = BOOL_TAG as i64;
pub(crate) const JS_NULL: i64 = NULL_TAG as i64;
pub(crate) const JS_UNDEF: i64 = UNDEF_TAG as i64;

pub(crate) fn js_number_bits(v: f64) -> i64 {
    i64::from_ne_bytes(v.to_ne_bytes())
}

pub struct CodeGen {
    pub(crate) functions: Vec<String>,
    pub(crate) current_fn: String,
    pub(crate) next_reg: u32,
    pub(crate) next_label: u32,
    pub(crate) next_str: u32,
    pub(crate) next_anon_fn: u32,
    pub(crate) scopes: Vec<HashMap<String, String>>,
    pub(crate) var_counter: u32,
    pub(crate) string_constants: Vec<(String, String, usize)>, // (global_name, escaped, byte_len)
    pub(crate) known_functions: HashMap<String, String>,        // js_name -> llvm_name
    pub(crate) block_terminated: bool,
    pub(crate) current_block: String,
    pub(crate) is_main: bool,
    pub(crate) loop_stack: Vec<(String, String)>, // (break_label, continue_label)
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
            loop_stack: Vec::new(),
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

    pub(crate) fn fresh_reg(&mut self) -> String {
        let r = format!("%t{}", self.next_reg);
        self.next_reg += 1;
        r
    }

    pub(crate) fn fresh_label(&mut self, prefix: &str) -> String {
        let l = format!("{}.{}", prefix, self.next_label);
        self.next_label += 1;
        l
    }

    pub(crate) fn emit(&mut self, line: &str) {
        writeln!(self.current_fn, "{}", line).unwrap();
    }

    pub(crate) fn emit_label(&mut self, label: &str) {
        writeln!(self.current_fn, "{}:", label).unwrap();
        self.current_block = label.to_string();
        self.block_terminated = false;
    }

    pub(crate) fn emit_br(&mut self, label: &str) {
        if !self.block_terminated {
            self.emit(&format!("  br label %{}", label));
            self.block_terminated = true;
        }
    }

    pub(crate) fn emit_cond_br(&mut self, cond: &str, then_l: &str, else_l: &str) {
        if !self.block_terminated {
            self.emit(&format!(
                "  br i1 {}, label %{}, label %{}",
                cond, then_l, else_l
            ));
            self.block_terminated = true;
        }
    }

    pub(crate) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub(crate) fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub(crate) fn declare_var(&mut self, name: &str) -> String {
        let mangled = format!("%js.{}.{}", name, self.var_counter);
        self.var_counter += 1;
        self.emit(&format!("  {} = alloca i64, align 8", mangled));
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), mangled.clone());
        }
        mangled
    }

    pub(crate) fn lookup_var(&self, name: &str) -> &str {
        for scope in self.scopes.iter().rev() {
            if let Some(reg) = scope.get(name) {
                return reg;
            }
        }
        panic!("undefined variable: {}", name);
    }

    pub(crate) fn intern_string(&mut self, s: &str) -> String {
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
    pub(crate) fn emit_string_const(&mut self, s: &str) -> String {
        let global = self.intern_string(s);
        let reg = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_string_from_cstr(ptr {})",
            reg, global
        ));
        reg
    }

    /// Convert a JSValue (i64) to a boolean (i1) for branching
    pub(crate) fn to_bool(&mut self, val: &str) -> String {
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
            // try/catch
            "declare ptr @js_try_get_buf()",
            "declare void @js_try_exit()",
            "declare i64 @js_get_error()",
            "declare i32 @_setjmp(ptr)",
            // JSON.parse
            "declare i64 @js_json_parse(i64)",
            // spread
            "declare void @js_array_concat_into(i64, i64)",
            "declare void @js_object_spread(i64, i64)",
            // this binding
            "declare i64 @js_this_get()",
            // bitwise operators
            "declare i64 @js_bitand(i64, i64)",
            "declare i64 @js_bitor(i64, i64)",
            "declare i64 @js_bitxor(i64, i64)",
            "declare i64 @js_shl(i64, i64)",
            "declare i64 @js_shr(i64, i64)",
            "declare i64 @js_ushr(i64, i64)",
            "declare i64 @js_bitnot(i64)",
            // in / instanceof / delete
            "declare i64 @js_in(i64, i64)",
            "declare i64 @js_instanceof(i64, i64)",
            "declare i64 @js_delete_prop(i64, i64)",
            // for-in
            "declare i64 @js_object_keys_or_indices(i64)",
            // null check for ??
            "declare i32 @js_is_nullish(i64)",
            // array.sort / splice
            "declare i64 @js_array_sort(i64, i64)",
            "declare i64 @js_array_splice(i64, ptr, i32)",
            // Object.entries / Object.assign
            "declare i64 @js_object_entries(i64)",
            "declare i64 @js_object_assign(i64, i64)",
            // Array.from
            "declare i64 @js_array_from(i64)",
            // fetch
            "declare i64 @js_fetch(i64, i64)",
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
}
