use oxc_ast::ast::*;
use oxc_syntax::operator::{BinaryOperator, LogicalOperator, UnaryOperator, UpdateOperator};

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

    pub(crate) fn emit_unary(&mut self, expr: &UnaryExpression<'_>) -> String {
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

    pub(crate) fn emit_logical(&mut self, expr: &LogicalExpression<'_>) -> String {
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

    pub(crate) fn emit_assignment(&mut self, expr: &AssignmentExpression<'_>) -> String {
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

    pub(crate) fn emit_conditional(&mut self, expr: &ConditionalExpression<'_>) -> String {
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
