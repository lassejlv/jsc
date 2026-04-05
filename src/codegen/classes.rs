use oxc_ast::ast::*;

use super::{CodeGen, JS_UNDEF};

/// Metadata for a compiled class
pub(crate) struct ClassInfo {
    pub constructor_fn: Option<String>, // LLVM fn name (indirect convention)
    pub prop_init_fn: Option<String>,   // LLVM fn name for property initializers
    pub methods: Vec<(String, String)>, // (js_name, llvm_fn_name)
    pub static_methods: Vec<(String, String)>,
    pub getters: Vec<(String, String)>,
    pub setters: Vec<(String, String)>,
    pub super_class: Option<String>,
}

impl CodeGen {
    /// Compile a class declaration and register its metadata
    pub(crate) fn emit_class_decl(&mut self, class: &Class<'_>) {
        let class_name = class
            .id
            .as_ref()
            .map(|id| id.name.as_str().to_string())
            .unwrap_or_else(|| {
                let n = format!("__AnonClass{}", self.next_anon_fn);
                self.next_anon_fn += 1;
                n
            });

        let info = self.compile_class_info(&class_name, class);
        self.class_info.insert(class_name.clone(), info);

        // Create a class constructor object that can be passed around / called with new
        let class_obj = self.fresh_reg();
        self.emit(&format!("  {} = call i64 @js_object_new()", class_obj));

        let type_key = self.emit_string_const("__type");
        let type_val = self.emit_string_const("Class");
        self.emit(&format!(
            "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
            class_obj, type_key, type_val
        ));
        let name_key = self.emit_string_const("__className");
        let name_val = self.emit_string_const(&class_name);
        self.emit(&format!(
            "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
            class_obj, name_key, name_val
        ));

        // Attach static methods to the class object itself
        if let Some(info) = self.class_info.get(&class_name) {
            for (js_name, llvm_name) in info.static_methods.clone() {
                let fval = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i64 @js_func_new(ptr @{}, ptr null, i32 0)",
                    fval, llvm_name
                ));
                let key = self.emit_string_const(&js_name);
                self.emit(&format!(
                    "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                    class_obj, key, fval
                ));
            }
        }

        let alloca = self.declare_var(&class_name);
        self.emit(&format!(
            "  store i64 {}, ptr {}, align 8",
            class_obj, alloca
        ));
    }

    /// Compile a class expression (returns the class constructor object)
    pub(crate) fn emit_class_expr(&mut self, class: &Class<'_>) -> String {
        let class_name = class
            .id
            .as_ref()
            .map(|id| id.name.as_str().to_string())
            .unwrap_or_else(|| {
                let n = format!("__AnonClass{}", self.next_anon_fn);
                self.next_anon_fn += 1;
                n
            });

        let info = self.compile_class_info(&class_name, class);
        self.class_info.insert(class_name.clone(), info);

        // Same as decl but return the object instead of storing in variable
        let class_obj = self.fresh_reg();
        self.emit(&format!("  {} = call i64 @js_object_new()", class_obj));
        let type_key = self.emit_string_const("__type");
        let type_val = self.emit_string_const("Class");
        self.emit(&format!(
            "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
            class_obj, type_key, type_val
        ));
        let name_key = self.emit_string_const("__className");
        let name_val = self.emit_string_const(&class_name);
        self.emit(&format!(
            "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
            class_obj, name_key, name_val
        ));

        if let Some(info) = self.class_info.get(&class_name) {
            for (js_name, llvm_name) in info.static_methods.clone() {
                let fval = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i64 @js_func_new(ptr @{}, ptr null, i32 0)",
                    fval, llvm_name
                ));
                let key = self.emit_string_const(&js_name);
                self.emit(&format!(
                    "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                    class_obj, key, fval
                ));
            }
        }

        class_obj
    }

    /// Extract class metadata by compiling method bodies
    fn compile_class_info(&mut self, class_name: &str, class: &Class<'_>) -> ClassInfo {
        let super_class = class.super_class.as_ref().and_then(|sc| {
            if let Expression::Identifier(id) = sc {
                Some(id.name.as_str().to_string())
            } else {
                None
            }
        });

        let mut info = ClassInfo {
            constructor_fn: None,
            prop_init_fn: None,
            methods: Vec::new(),
            static_methods: Vec::new(),
            getters: Vec::new(),
            setters: Vec::new(),
            super_class,
        };

        // Collect property definitions that have initializers — compile as an init function
        // that takes (ptr %args, i32 %argc, ptr %closure) and uses this from the this-stack
        let mut has_prop_inits = false;
        for element in &class.body.body {
            if let ClassElement::PropertyDefinition(prop) = element {
                if prop.value.is_some() && !prop.r#static {
                    has_prop_inits = true;
                    break;
                }
            }
        }

        if has_prop_inits {
            // Compile a property initializer function
            let saved_fn = std::mem::take(&mut self.current_fn);
            let saved_scopes = std::mem::take(&mut self.scopes);
            let saved_terminated = self.block_terminated;
            let saved_block = std::mem::take(&mut self.current_block);
            let saved_is_main = self.is_main;

            let init_fn_name = format!("__jsfn_propinit_{}", self.next_anon_fn);
            self.next_anon_fn += 1;

            self.scopes = vec![std::collections::HashMap::new()];
            self.block_terminated = false;
            self.current_block = "entry".to_string();
            self.is_main = false;

            self.emit(&format!(
                "define i64 @{}(ptr %args, i32 %argc, ptr %closure) {{",
                init_fn_name
            ));
            self.emit("entry:");

            for element in &class.body.body {
                if let ClassElement::PropertyDefinition(prop) = element {
                    if prop.r#static {
                        continue;
                    }
                    let prop_name = match &prop.key {
                        PropertyKey::StaticIdentifier(id) => id.name.as_str().to_string(),
                        PropertyKey::StringLiteral(s) => s.value.as_str().to_string(),
                        PropertyKey::PrivateIdentifier(id) => {
                            format!("__priv_{}", id.name.as_str())
                        }
                        _ => continue,
                    };
                    if let Some(init_expr) = &prop.value {
                        let val = self.emit_expression(init_expr);
                        // this.prop = val
                        let this_val = self.fresh_reg();
                        self.emit(&format!("  {} = call i64 @js_this_get()", this_val));
                        let key = self.emit_string_const(&prop_name);
                        self.emit(&format!(
                            "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                            this_val, key, val
                        ));
                    }
                }
            }

            self.emit(&format!("  ret i64 {}", JS_UNDEF));
            self.emit("}");
            self.functions.push(std::mem::take(&mut self.current_fn));

            self.current_fn = saved_fn;
            self.scopes = saved_scopes;
            self.block_terminated = saved_terminated;
            self.current_block = saved_block;
            self.is_main = saved_is_main;

            info.prop_init_fn = Some(init_fn_name);
        }

        for element in &class.body.body {
            match element {
                ClassElement::MethodDefinition(method) => {
                    let method_name = match &method.key {
                        PropertyKey::StaticIdentifier(id) => id.name.as_str().to_string(),
                        PropertyKey::StringLiteral(s) => s.value.as_str().to_string(),
                        PropertyKey::PrivateIdentifier(id) => {
                            format!("__priv_{}", id.name.as_str())
                        }
                        _ => continue,
                    };

                    let func = &method.value;
                    let fn_body = match &func.body {
                        Some(body) => body,
                        None => continue,
                    };

                    // Compile the method body as an anonymous function
                    // emit_arrow_fn returns a register with js_func_new result,
                    // but more importantly it defines the LLVM function.
                    // The function name is __jsfn_anon_{next_anon_fn - 1} after the call.
                    let _reg = self.emit_arrow_fn(&func.params, fn_body, false, func.r#async);
                    let fn_name = format!("__jsfn_anon_{}", self.next_anon_fn - 1);

                    match method.kind {
                        MethodDefinitionKind::Constructor => {
                            info.constructor_fn = Some(fn_name);
                        }
                        MethodDefinitionKind::Method => {
                            if method.r#static {
                                info.static_methods.push((method_name, fn_name));
                            } else {
                                info.methods.push((method_name, fn_name));
                            }
                        }
                        MethodDefinitionKind::Get => {
                            info.getters.push((method_name, fn_name));
                        }
                        MethodDefinitionKind::Set => {
                            info.setters.push((method_name, fn_name));
                        }
                    }
                }
                ClassElement::PropertyDefinition(prop) => {
                    // Property definitions with initializers are handled in the constructor
                    // via `this.prop = value`. We skip them here.
                    let _ = prop;
                }
                _ => {}
            }
        }

        info
    }

    /// Emit code for `new ClassName(args)` when ClassName is a known class
    pub(crate) fn emit_new_class(
        &mut self,
        class_name: &str,
        arguments: &[Argument<'_>],
    ) -> String {
        // Clone info to avoid borrow conflict with self
        let info = match self.class_info.get(class_name).map(|i| ClassInfo {
            constructor_fn: i.constructor_fn.clone(),
            prop_init_fn: i.prop_init_fn.clone(),
            methods: i.methods.clone(),
            static_methods: i.static_methods.clone(),
            getters: i.getters.clone(),
            setters: i.setters.clone(),
            super_class: i.super_class.clone(),
        }) {
            Some(i) => i,
            None => return format!("{}", JS_UNDEF),
        };

        // 1. Create instance
        let instance = self.fresh_reg();
        self.emit(&format!("  {} = call i64 @js_object_new()", instance));

        // Set __type
        let type_key = self.emit_string_const("__type");
        let type_val = self.emit_string_const(class_name);
        self.emit(&format!(
            "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
            instance, type_key, type_val
        ));

        // 2. If extends, create super instance and copy its properties/methods
        if let Some(ref super_name) = info.super_class {
            let super_inst = self.emit_new_class(super_name, &[]);
            self.emit(&format!(
                "  call i64 @js_object_assign(i64 {}, i64 {})",
                instance, super_inst
            ));
            // Restore __type to derived class
            let tk = self.emit_string_const("__type");
            let tv = self.emit_string_const(class_name);
            self.emit(&format!(
                "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                instance, tk, tv
            ));
        }

        // 3. Stamp instance methods
        for (js_name, llvm_name) in &info.methods {
            let fval = self.fresh_reg();
            self.emit(&format!(
                "  {} = call i64 @js_func_new(ptr @{}, ptr null, i32 0)",
                fval, llvm_name
            ));
            let key = self.emit_string_const(js_name);
            self.emit(&format!(
                "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                instance, key, fval
            ));
        }

        // 4. Stamp getters
        if !info.getters.is_empty() {
            let getters_obj = self.fresh_reg();
            self.emit(&format!("  {} = call i64 @js_object_new()", getters_obj));
            for (js_name, llvm_name) in &info.getters {
                let fval = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i64 @js_func_new(ptr @{}, ptr null, i32 0)",
                    fval, llvm_name
                ));
                let key = self.emit_string_const(js_name);
                self.emit(&format!(
                    "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                    getters_obj, key, fval
                ));
            }
            let gk = self.emit_string_const("__getters");
            self.emit(&format!(
                "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                instance, gk, getters_obj
            ));
        }

        // 5. Stamp setters
        if !info.setters.is_empty() {
            let setters_obj = self.fresh_reg();
            self.emit(&format!("  {} = call i64 @js_object_new()", setters_obj));
            for (js_name, llvm_name) in &info.setters {
                let fval = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i64 @js_func_new(ptr @{}, ptr null, i32 0)",
                    fval, llvm_name
                ));
                let key = self.emit_string_const(js_name);
                self.emit(&format!(
                    "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                    setters_obj, key, fval
                ));
            }
            let sk = self.emit_string_const("__setters");
            self.emit(&format!(
                "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                instance, sk, setters_obj
            ));
        }

        // 6. Push this for constructor and prop initializers
        self.emit(&format!("  call void @js_this_push(i64 {})", instance));

        // 6a. Run property initializers (before constructor so constructor can override)
        if let Some(ref init_fn) = info.prop_init_fn {
            let init_fval = self.fresh_reg();
            self.emit(&format!(
                "  {} = call i64 @js_func_new(ptr @{}, ptr null, i32 0)",
                init_fval, init_fn
            ));
            self.emit(&format!(
                "  call i64 @js_call_func(i64 {}, ptr null, i32 0)",
                init_fval
            ));
        }

        // 6b. Call constructor
        if let Some(ref ctor_fn) = info.constructor_fn {
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

            // Create a temporary function value for the constructor
            let ctor_fval = self.fresh_reg();
            self.emit(&format!(
                "  {} = call i64 @js_func_new(ptr @{}, ptr null, i32 {})",
                ctor_fval, ctor_fn, argc
            ));

            self.emit(&format!(
                "  call i64 @js_call_func(i64 {}, ptr {}, i32 {})",
                ctor_fval, args_alloca, argc
            ));
        }

        self.emit("  call void @js_this_pop()");
        instance
    }
}
