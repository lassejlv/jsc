use oxc_ast::ast::*;
use oxc_syntax::operator::{
    AssignmentOperator, BinaryOperator, LogicalOperator, UnaryOperator, UpdateOperator,
};

use super::{js_number_bits, CodeGen, JS_FALSE, JS_NULL, JS_TRUE, JS_UNDEF};

impl CodeGen {
    pub(crate) fn emit_expression(&mut self, expr: &Expression<'_>) -> String {
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
                self.emit_arrow_fn(&af.params, af.body.as_ref(), af.expression, af.r#async)
            }
            Expression::FunctionExpression(fe) => {
                if let Some(body) = &fe.body {
                    self.emit_arrow_fn(&fe.params, body, false, fe.r#async)
                } else {
                    format!("{}", JS_UNDEF)
                }
            }
            Expression::ThisExpression(_) => {
                let reg = self.fresh_reg();
                self.emit(&format!("  {} = call i64 @js_this_get()", reg));
                reg
            }
            Expression::NewExpression(ne) => self.emit_new(ne),
            Expression::SequenceExpression(se) => {
                let mut last = format!("{}", JS_UNDEF);
                for expr in &se.expressions {
                    last = self.emit_expression(expr);
                }
                last
            }
            Expression::ChainExpression(ce) => self.emit_chain(&ce.expression),
            Expression::AwaitExpression(ae) => {
                let val = self.emit_expression(&ae.argument);
                let reg = self.fresh_reg();
                self.emit(&format!("  {} = call i64 @js_await(i64 {})", reg, val));
                reg
            }
            _ => format!("{}", JS_UNDEF),
        }
    }

    pub(crate) fn emit_binary(&mut self, expr: &BinaryExpression<'_>) -> String {
        let left = self.emit_expression(&expr.left);
        let right = self.emit_expression(&expr.right);
        let reg = self.fresh_reg();

        let func = match expr.operator {
            BinaryOperator::Addition => "js_add",
            BinaryOperator::Subtraction => "js_sub",
            BinaryOperator::Multiplication => "js_mul",
            BinaryOperator::Division => "js_div",
            BinaryOperator::Remainder => "js_mod",
            BinaryOperator::Exponential => "js_math_pow",
            BinaryOperator::LessThan => "js_lt",
            BinaryOperator::GreaterThan => "js_gt",
            BinaryOperator::LessEqualThan => "js_lte",
            BinaryOperator::GreaterEqualThan => "js_gte",
            BinaryOperator::Equality => "js_eq",
            BinaryOperator::Inequality => "js_neq",
            BinaryOperator::StrictEquality => "js_seq",
            BinaryOperator::StrictInequality => "js_sneq",
            BinaryOperator::BitwiseAnd => "js_bitand",
            BinaryOperator::BitwiseOR => "js_bitor",
            BinaryOperator::BitwiseXOR => "js_bitxor",
            BinaryOperator::ShiftLeft => "js_shl",
            BinaryOperator::ShiftRight => "js_shr",
            BinaryOperator::ShiftRightZeroFill => "js_ushr",
            BinaryOperator::In => "js_in",
            BinaryOperator::Instanceof => "js_instanceof",
        };

        self.emit(&format!(
            "  {} = call i64 @{}(i64 {}, i64 {})",
            reg, func, left, right
        ));
        reg
    }

    pub(crate) fn emit_unary(&mut self, expr: &UnaryExpression<'_>) -> String {
        if expr.operator == UnaryOperator::Typeof {
            let val = self.emit_expression(&expr.argument);
            let reg = self.fresh_reg();
            self.emit(&format!(
                "  {} = call i64 @js_typeof_val(i64 {})",
                reg, val
            ));
            return reg;
        }
        if expr.operator == UnaryOperator::Void {
            self.emit_expression(&expr.argument); // evaluate for side effects
            return format!("{}", JS_UNDEF);
        }
        if expr.operator == UnaryOperator::Delete {
            return self.emit_delete(&expr.argument);
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
            UnaryOperator::BitwiseNot => {
                self.emit(&format!("  {} = call i64 @js_bitnot(i64 {})", reg, operand));
            }
            _ => {
                return format!("{}", JS_UNDEF);
            }
        }
        reg
    }

    fn emit_delete(&mut self, expr: &Expression<'_>) -> String {
        match expr {
            Expression::StaticMemberExpression(sme) => {
                let obj = self.emit_expression(&sme.object);
                let key = self.emit_string_const(sme.property.name.as_str());
                let reg = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i64 @js_delete_prop(i64 {}, i64 {})",
                    reg, obj, key
                ));
                reg
            }
            Expression::ComputedMemberExpression(cme) => {
                let obj = self.emit_expression(&cme.object);
                let key = self.emit_expression(&cme.expression);
                let reg = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i64 @js_delete_prop(i64 {}, i64 {})",
                    reg, obj, key
                ));
                reg
            }
            _ => {
                self.emit_expression(expr); // evaluate for side effects
                format!("{}", JS_TRUE) // delete on non-property always returns true
            }
        }
    }

    pub(crate) fn emit_logical(&mut self, expr: &LogicalExpression<'_>) -> String {
        let left = self.emit_expression(&expr.left);
        let left_block = self.current_block.clone();

        let rhs_label = self.fresh_label("logic.rhs");
        let end_label = self.fresh_label("logic.end");

        match expr.operator {
            LogicalOperator::And => {
                let left_bool = self.to_bool(&left);
                self.emit_cond_br(&left_bool, &rhs_label, &end_label);
            }
            LogicalOperator::Or => {
                let left_bool = self.to_bool(&left);
                self.emit_cond_br(&left_bool, &end_label, &rhs_label);
            }
            LogicalOperator::Coalesce => {
                // ?? — check for null/undefined, NOT truthiness
                let nullish_i32 = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i32 @js_is_nullish(i64 {})",
                    nullish_i32, left
                ));
                let is_nullish = self.fresh_reg();
                self.emit(&format!(
                    "  {} = trunc i32 {} to i1",
                    is_nullish, nullish_i32
                ));
                self.emit_cond_br(&is_nullish, &rhs_label, &end_label);
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

    pub(crate) fn emit_assignment(&mut self, expr: &AssignmentExpression<'_>) -> String {
        // For compound assignments, compute the new value from old + right
        let compute_compound = |cg: &mut Self, old_val: &str, right_val: &str, op: &AssignmentOperator| -> String {
            let func = match op {
                AssignmentOperator::Addition => "js_add",
                AssignmentOperator::Subtraction => "js_sub",
                AssignmentOperator::Multiplication => "js_mul",
                AssignmentOperator::Division => "js_div",
                AssignmentOperator::Remainder => "js_mod",
                AssignmentOperator::Exponential => "js_math_pow",
                AssignmentOperator::BitwiseAnd => "js_bitand",
                AssignmentOperator::BitwiseOR => "js_bitor",
                AssignmentOperator::BitwiseXOR => "js_bitxor",
                AssignmentOperator::ShiftLeft => "js_shl",
                AssignmentOperator::ShiftRight => "js_shr",
                AssignmentOperator::ShiftRightZeroFill => "js_ushr",
                _ => return right_val.to_string(), // logical assign handled separately
            };
            let r = cg.fresh_reg();
            cg.emit(&format!(
                "  {} = call i64 @{}(i64 {}, i64 {})",
                r, func, old_val, right_val
            ));
            r
        };

        let is_simple = expr.operator == AssignmentOperator::Assign;

        match &expr.left {
            AssignmentTarget::AssignmentTargetIdentifier(id) => {
                let alloca = self.lookup_var(id.name.as_str()).to_string();
                let val = if is_simple {
                    self.emit_expression(&expr.right)
                } else {
                    let old = self.fresh_reg();
                    self.emit(&format!("  {} = load i64, ptr {}, align 8", old, alloca));
                    let right = self.emit_expression(&expr.right);
                    compute_compound(self, &old, &right, &expr.operator)
                };
                self.emit(&format!("  store i64 {}, ptr {}, align 8", val, alloca));
                val
            }
            AssignmentTarget::StaticMemberExpression(sme) => {
                let obj = self.emit_expression(&sme.object);
                let key = self.emit_string_const(sme.property.name.as_str());
                let val = if is_simple {
                    self.emit_expression(&expr.right)
                } else {
                    let old = self.fresh_reg();
                    self.emit(&format!(
                        "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
                        old, obj, key
                    ));
                    let right = self.emit_expression(&expr.right);
                    compute_compound(self, &old, &right, &expr.operator)
                };
                self.emit(&format!(
                    "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                    obj, key, val
                ));
                val
            }
            AssignmentTarget::ComputedMemberExpression(cme) => {
                let obj = self.emit_expression(&cme.object);
                let key = self.emit_expression(&cme.expression);
                let val = if is_simple {
                    self.emit_expression(&expr.right)
                } else {
                    let old = self.fresh_reg();
                    self.emit(&format!(
                        "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
                        old, obj, key
                    ));
                    let right = self.emit_expression(&expr.right);
                    compute_compound(self, &old, &right, &expr.operator)
                };
                self.emit(&format!(
                    "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                    obj, key, val
                ));
                val
            }
            _ => {
                // Unsupported target
                self.emit_expression(&expr.right)
            }
        }
    }

    /// Optional chaining: obj?.prop, obj?.method(), obj?.[key]
    fn emit_chain(&mut self, expr: &ChainElement<'_>) -> String {
        match expr {
            ChainElement::StaticMemberExpression(sme) => {
                let obj = self.emit_expression(&sme.object);
                let is_null = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i32 @js_is_nullish(i64 {})",
                    is_null, obj
                ));
                let cond = self.fresh_reg();
                self.emit(&format!("  {} = trunc i32 {} to i1", cond, is_null));

                let null_label = self.fresh_label("chain.null");
                let ok_label = self.fresh_label("chain.ok");
                let end_label = self.fresh_label("chain.end");

                self.emit_cond_br(&cond, &null_label, &ok_label);

                self.emit_label(&null_label);
                let null_block = self.current_block.clone();
                self.emit_br(&end_label);

                self.emit_label(&ok_label);
                let key = self.emit_string_const(sme.property.name.as_str());
                let val = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
                    val, obj, key
                ));
                let ok_block = self.current_block.clone();
                self.emit_br(&end_label);

                self.emit_label(&end_label);
                let result = self.fresh_reg();
                self.emit(&format!(
                    "  {} = phi i64 [ {}, %{} ], [ {}, %{} ]",
                    result, JS_UNDEF, null_block, val, ok_block
                ));
                result
            }
            ChainElement::ComputedMemberExpression(cme) => {
                let obj = self.emit_expression(&cme.object);
                let is_null = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i32 @js_is_nullish(i64 {})",
                    is_null, obj
                ));
                let cond = self.fresh_reg();
                self.emit(&format!("  {} = trunc i32 {} to i1", cond, is_null));

                let null_label = self.fresh_label("chain.null");
                let ok_label = self.fresh_label("chain.ok");
                let end_label = self.fresh_label("chain.end");

                self.emit_cond_br(&cond, &null_label, &ok_label);

                self.emit_label(&null_label);
                let null_block = self.current_block.clone();
                self.emit_br(&end_label);

                self.emit_label(&ok_label);
                let key = self.emit_expression(&cme.expression);
                let val = self.fresh_reg();
                self.emit(&format!(
                    "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
                    val, obj, key
                ));
                let ok_block = self.current_block.clone();
                self.emit_br(&end_label);

                self.emit_label(&end_label);
                let result = self.fresh_reg();
                self.emit(&format!(
                    "  {} = phi i64 [ {}, %{} ], [ {}, %{} ]",
                    result, JS_UNDEF, null_block, val, ok_block
                ));
                result
            }
            ChainElement::CallExpression(ce) => {
                // obj?.method() — for now, just emit normal call
                self.emit_call(ce)
            }
            _ => format!("{}", JS_UNDEF),
        }
    }

    pub(crate) fn emit_conditional(&mut self, expr: &ConditionalExpression<'_>) -> String {
        let cond = self.emit_expression(&expr.test);
        let cond_bool = self.to_bool(&cond);

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

    pub(crate) fn emit_update(&mut self, expr: &UpdateExpression<'_>) -> String {
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

    pub(crate) fn emit_new(&mut self, ne: &NewExpression<'_>) -> String {
        if let Expression::Identifier(id) = &ne.callee {
            match id.name.as_str() {
                "Error" => {
                    let obj = self.fresh_reg();
                    self.emit(&format!("  {} = call i64 @js_object_new()", obj));
                    let key = self.emit_string_const("name");
                    let name_val = self.emit_string_const("Error");
                    self.emit(&format!(
                        "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                        obj, key, name_val
                    ));
                    let msg_key = self.emit_string_const("message");
                    let msg_val = if ne.arguments.is_empty() {
                        self.emit_string_const("")
                    } else {
                        self.emit_call_arg(&ne.arguments[0])
                    };
                    self.emit(&format!(
                        "  call void @js_set_prop(i64 {}, i64 {}, i64 {})",
                        obj, msg_key, msg_val
                    ));
                    return obj;
                }
                "Promise" => {
                    let executor = if ne.arguments.is_empty() {
                        format!("{}", JS_UNDEF)
                    } else {
                        self.emit_call_arg(&ne.arguments[0])
                    };
                    let reg = self.fresh_reg();
                    self.emit(&format!(
                        "  {} = call i64 @js_promise_create(i64 {})",
                        reg, executor
                    ));
                    return reg;
                }
                _ => {}
            }
        }
        format!("{}", JS_UNDEF)
    }

    pub(crate) fn emit_call_arg(&mut self, arg: &Argument<'_>) -> String {
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
                self.emit_arrow_fn(&af.params, af.body.as_ref(), af.expression, af.r#async)
            }
            Argument::FunctionExpression(fe) => {
                if let Some(body) = &fe.body {
                    self.emit_arrow_fn(&fe.params, body, false, fe.r#async)
                } else {
                    format!("{}", JS_UNDEF)
                }
            }
            _ => format!("{}", JS_UNDEF),
        }
    }
}
