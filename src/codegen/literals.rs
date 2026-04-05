use oxc_ast::ast::*;

use super::{js_number_bits, CodeGen, JS_UNDEF};

impl CodeGen {
    pub(crate) fn emit_object(&mut self, expr: &ObjectExpression<'_>) -> String {
        let obj = self.fresh_reg();
        self.emit(&format!("  {} = call i64 @js_object_new()", obj));

        for prop in &expr.properties {
            match prop {
                ObjectPropertyKind::ObjectProperty(p) => {
                    let key = match &p.key {
                        PropertyKey::StaticIdentifier(id) => {
                            self.emit_string_const(id.name.as_str())
                        }
                        PropertyKey::StringLiteral(s) => self.emit_string_const(s.value.as_str()),
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
                ObjectPropertyKind::SpreadProperty(spread) => {
                    let source = self.emit_expression(&spread.argument);
                    self.emit(&format!(
                        "  call void @js_object_spread(i64 {}, i64 {})",
                        obj, source
                    ));
                }
            }
        }
        obj
    }

    pub(crate) fn emit_array(&mut self, expr: &ArrayExpression<'_>) -> String {
        let arr = self.fresh_reg();
        self.emit(&format!("  {} = call i64 @js_array_new()", arr));

        for elem in &expr.elements {
            match elem {
                ArrayExpressionElement::SpreadElement(spread) => {
                    let source = self.emit_expression(&spread.argument);
                    self.emit(&format!(
                        "  call void @js_array_concat_into(i64 {}, i64 {})",
                        arr, source
                    ));
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

    pub(crate) fn emit_static_member(&mut self, sme: &StaticMemberExpression<'_>) -> String {
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

    pub(crate) fn emit_computed_member(&mut self, cme: &ComputedMemberExpression<'_>) -> String {
        let obj = self.emit_expression(&cme.object);
        let key = self.emit_expression(&cme.expression);
        let result = self.fresh_reg();
        self.emit(&format!(
            "  {} = call i64 @js_get_prop(i64 {}, i64 {})",
            result, obj, key
        ));
        result
    }

    pub(crate) fn emit_template_literal(&mut self, tl: &TemplateLiteral<'_>) -> String {
        // Start with the first quasi
        let first_quasi = &tl.quasis[0];
        let mut result = self.emit_string_const(first_quasi.value.raw.as_str());

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
}
