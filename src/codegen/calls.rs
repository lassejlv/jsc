use oxc_ast::ast::*;

use super::{CodeGen, JS_UNDEF};

impl CodeGen {
    pub(crate) fn emit_call(&mut self, expr: &CallExpression<'_>) -> String {
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

    pub(crate) fn emit_indirect_call(&mut self, func_val: &str, arguments: &[Argument<'_>]) -> String {
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

                // JSON.parse
                if obj_name == "JSON" && method == "parse" {
                    let arg = if expr.arguments.is_empty() {
                        format!("{}", JS_UNDEF)
                    } else {
                        self.emit_call_arg(&expr.arguments[0])
                    };
                    let r = self.fresh_reg();
                    self.emit(&format!(
                        "  {} = call i64 @js_json_parse(i64 {})",
                        r, arg
                    ));
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

    pub(crate) fn emit_method_call(
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
}
