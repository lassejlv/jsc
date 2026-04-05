// ============================================================
// Reference counting
// ============================================================

void js_release(JSValue v) {
    if (!js_is_string(v) && !js_is_object(v) && !js_is_array(v) && !js_is_function(v)) return;
    HeapHeader* h = (HeapHeader*)js_as_ptr(v);
    if (--h->refcount <= 0) {
        if (js_is_string(v)) {
            JSString* s = (JSString*)h;
            free(s->data); free(s);
        } else if (js_is_object(v)) {
            JSObject* obj = (JSObject*)h;
            for (int i = 0; i < obj->capacity; i++) {
                if (obj->entries[i].occupied == 1) {
                    free(obj->entries[i].key);
                    js_release(obj->entries[i].value);
                }
            }
            free(obj->entries); free(obj);
        } else if (js_is_array(v)) {
            JSArray* arr = (JSArray*)h;
            for (int i = 0; i < arr->length; i++) js_release(arr->data[i]);
            free(arr->data); free(arr);
        } else if (js_is_function(v)) {
            free(h);
        }
    }
}

// ============================================================
// Type coercion
// ============================================================

double js_to_number(JSValue v) {
    if (js_is_number(v)) return js_as_number(v);
    if (js_is_bool(v)) return js_as_bool(v) ? 1.0 : 0.0;
    if (js_is_null(v)) return 0.0;
    if (js_is_undefined(v)) return NAN;
    if (js_is_string(v)) {
        JSString* s = js_as_string(v);
        if (s->length == 0) return 0.0;
        char* end;
        double r = strtod(s->data, &end);
        while (*end == ' ' || *end == '\t' || *end == '\n' || *end == '\r') end++;
        if (*end != '\0') return NAN;
        return r;
    }
    return NAN;
}

JSValue js_to_number_val(JSValue v) { return js_number(js_to_number(v)); }

int js_to_boolean(JSValue v) {
    if (js_is_number(v)) { double n = js_as_number(v); return n != 0.0 && !isnan(n); }
    if (js_is_bool(v)) return js_as_bool(v);
    if (js_is_null(v) || js_is_undefined(v)) return 0;
    if (js_is_string(v)) return js_as_string(v)->length > 0;
    return 1; // objects, arrays, functions are truthy
}

int js_is_truthy(JSValue v) { return js_to_boolean(v); }

// Number to string with shortest representation (matches JS behavior)
static char* js_number_to_cstr(double n) {
    char buf[64];
    if (isnan(n)) return _strdup("NaN");
    if (isinf(n)) return _strdup(n > 0 ? "Infinity" : "-Infinity");
    if (n == 0.0) return _strdup("0");
    // Integer: no decimal point, no scientific notation for reasonable values
    if (n == floor(n) && fabs(n) < 1e20 && fabs(n) >= 1.0) {
        snprintf(buf, sizeof(buf), "%.0f", n);
        return _strdup(buf);
    }
    // Float: try increasing precision until roundtrip-safe
    for (int prec = 1; prec <= 21; prec++) {
        snprintf(buf, sizeof(buf), "%.*g", prec, n);
        if (strtod(buf, NULL) == n) return _strdup(buf);
    }
    snprintf(buf, sizeof(buf), "%.21g", n);
    return _strdup(buf);
}

char* js_to_cstring(JSValue v) {
    if (js_is_string(v)) return _strdup(js_as_string(v)->data);
    if (js_is_number(v)) return js_number_to_cstr(js_as_number(v));
    if (js_is_bool(v)) return _strdup(js_as_bool(v) ? "true" : "false");
    if (js_is_null(v)) return _strdup("null");
    if (js_is_undefined(v)) return _strdup("undefined");
    if (js_is_array(v)) {
        JSArray* arr = (JSArray*)js_as_ptr(v);
        if (arr->length == 0) return _strdup("");
        char** parts = (char**)malloc(arr->length * sizeof(char*));
        int total = 0;
        for (int i = 0; i < arr->length; i++) {
            parts[i] = js_to_cstring(arr->data[i]);
            total += (int)strlen(parts[i]);
        }
        total += arr->length - 1;
        char* r = (char*)malloc(total + 1);
        r[0] = '\0';
        for (int i = 0; i < arr->length; i++) {
            if (i > 0) strcat(r, ",");
            strcat(r, parts[i]);
            free(parts[i]);
        }
        free(parts);
        return r;
    }
    if (js_is_object(v)) return _strdup("[object Object]");
    if (js_is_function(v)) return _strdup("function() { [native code] }");
    return _strdup("undefined");
}

JSValue js_to_string_val(JSValue v) {
    if (js_is_string(v)) return v;
    char* s = js_to_cstring(v);
    JSValue r = js_string_from_cstr(s);
    free(s);
    return r;
}

// ============================================================
// Error handling
// ============================================================

#define MAX_TRY_DEPTH 64
static jmp_buf js_try_bufs[MAX_TRY_DEPTH];
static JSValue js_error_vals[MAX_TRY_DEPTH];
static int js_try_depth = 0;

void js_throw(JSValue error) {
    if (js_try_depth == 0) {
        fprintf(stderr, "Uncaught ");
        char* s = js_to_cstring(error);
        fprintf(stderr, "%s\n", s);
        free(s);
        exit(1);
    }
    js_try_depth--;
    js_error_vals[js_try_depth] = error;
    longjmp(js_try_bufs[js_try_depth], 1);
}

// js_try_enter: returns pointer to the jmp_buf to use, increments depth
void* js_try_get_buf(void) {
    if (js_try_depth >= MAX_TRY_DEPTH) {
        fprintf(stderr, "Error: try/catch nesting too deep\n");
        exit(1);
    }
    return &js_try_bufs[js_try_depth++];
}

void js_try_exit(void) {
    if (js_try_depth > 0) js_try_depth--;
}

JSValue js_get_error(void) {
    return js_error_vals[js_try_depth]; // depth was already decremented by longjmp path
}

JSValue js_error_new(const char* message) {
    JSValue obj = js_object_new();
    js_object_set((JSObject*)js_as_ptr(obj), "message", js_string_from_cstr(message));
    js_object_set((JSObject*)js_as_ptr(obj), "name", js_string_from_cstr("Error"));
    return obj;
}

// ============================================================
// Arithmetic operations
// ============================================================

JSValue js_add(JSValue a, JSValue b) {
    if (js_is_string(a) || js_is_string(b)) {
        JSValue sa = js_to_string_val(a);
        JSValue sb = js_to_string_val(b);
        JSValue r = js_string_concat(sa, sb);
        return r;
    }
    return js_number(js_to_number(a) + js_to_number(b));
}

JSValue js_sub(JSValue a, JSValue b) { return js_number(js_to_number(a) - js_to_number(b)); }
JSValue js_mul(JSValue a, JSValue b) { return js_number(js_to_number(a) * js_to_number(b)); }
JSValue js_div(JSValue a, JSValue b) { return js_number(js_to_number(a) / js_to_number(b)); }
JSValue js_mod(JSValue a, JSValue b) { return js_number(fmod(js_to_number(a), js_to_number(b))); }
JSValue js_neg(JSValue a)            { return js_number(-js_to_number(a)); }
JSValue js_not(JSValue a)            { return js_is_truthy(a) ? JS_FALSE : JS_TRUE; }
JSValue js_typeof_val(JSValue v) {
    if (js_is_number(v))    return js_string_from_cstr("number");
    if (js_is_bool(v))      return js_string_from_cstr("boolean");
    if (js_is_null(v))      return js_string_from_cstr("object"); // JS quirk
    if (js_is_undefined(v)) return js_string_from_cstr("undefined");
    if (js_is_string(v))    return js_string_from_cstr("string");
    if (js_is_function(v))  return js_string_from_cstr("function");
    return js_string_from_cstr("object");
}

// ============================================================
// Comparison operations
// ============================================================

static int js_abstract_eq(JSValue a, JSValue b) {
    if (js_is_number(a) && js_is_number(b)) return js_as_number(a) == js_as_number(b);
    if (js_is_string(a) && js_is_string(b)) return js_string_equals(js_as_string(a), js_as_string(b));
    if (js_is_bool(a) && js_is_bool(b)) return js_as_bool(a) == js_as_bool(b);
    if (js_is_null(a) && js_is_null(b)) return 1;
    if (js_is_undefined(a) && js_is_undefined(b)) return 1;
    if (js_is_null(a) && js_is_undefined(b)) return 1;
    if (js_is_undefined(a) && js_is_null(b)) return 1;
    if (js_is_number(a) && js_is_string(b)) return js_as_number(a) == js_to_number(b);
    if (js_is_string(a) && js_is_number(b)) return js_to_number(a) == js_as_number(b);
    if (js_is_bool(a)) return js_abstract_eq(js_number(js_as_bool(a) ? 1.0 : 0.0), b);
    if (js_is_bool(b)) return js_abstract_eq(a, js_number(js_as_bool(b) ? 1.0 : 0.0));
    return 0;
}

static int js_strict_eq(JSValue a, JSValue b) {
    if (js_is_number(a) && js_is_number(b)) return js_as_number(a) == js_as_number(b);
    if (js_is_string(a) && js_is_string(b)) return js_string_equals(js_as_string(a), js_as_string(b));
    return (uint64_t)a == (uint64_t)b;
}

JSValue js_eq(JSValue a, JSValue b)   { return js_abstract_eq(a, b) ? JS_TRUE : JS_FALSE; }
JSValue js_neq(JSValue a, JSValue b)  { return js_abstract_eq(a, b) ? JS_FALSE : JS_TRUE; }
JSValue js_seq(JSValue a, JSValue b)  { return js_strict_eq(a, b) ? JS_TRUE : JS_FALSE; }
JSValue js_sneq(JSValue a, JSValue b) { return js_strict_eq(a, b) ? JS_FALSE : JS_TRUE; }

JSValue js_lt(JSValue a, JSValue b) {
    if (js_is_string(a) && js_is_string(b))
        return js_string_compare(js_as_string(a), js_as_string(b)) < 0 ? JS_TRUE : JS_FALSE;
    return js_to_number(a) < js_to_number(b) ? JS_TRUE : JS_FALSE;
}
JSValue js_gt(JSValue a, JSValue b) {
    if (js_is_string(a) && js_is_string(b))
        return js_string_compare(js_as_string(a), js_as_string(b)) > 0 ? JS_TRUE : JS_FALSE;
    return js_to_number(a) > js_to_number(b) ? JS_TRUE : JS_FALSE;
}
JSValue js_lte(JSValue a, JSValue b) {
    if (js_is_string(a) && js_is_string(b))
        return js_string_compare(js_as_string(a), js_as_string(b)) <= 0 ? JS_TRUE : JS_FALSE;
    return js_to_number(a) <= js_to_number(b) ? JS_TRUE : JS_FALSE;
}
JSValue js_gte(JSValue a, JSValue b) {
    if (js_is_string(a) && js_is_string(b))
        return js_string_compare(js_as_string(a), js_as_string(b)) >= 0 ? JS_TRUE : JS_FALSE;
    return js_to_number(a) >= js_to_number(b) ? JS_TRUE : JS_FALSE;
}

// ============================================================
// Console I/O
// ============================================================

static char* js_format_for_log(JSValue v) {
    if (js_is_object(v) || js_is_array(v)) {
        JSValue json = js_json_stringify(v);
        return js_to_cstring(json);
    }
    return js_to_cstring(v);
}

void js_console_log(JSValue* args, int argc) {
    for (int i = 0; i < argc; i++) {
        if (i > 0) printf(" ");
        char* s = js_format_for_log(args[i]);
        printf("%s", s);
        free(s);
    }
    printf("\n");
    fflush(stdout);
}

void js_console_error(JSValue* args, int argc) {
    for (int i = 0; i < argc; i++) {
        if (i > 0) fprintf(stderr, " ");
        char* s = js_format_for_log(args[i]);
        fprintf(stderr, "%s", s);
        free(s);
    }
    fprintf(stderr, "\n");
    fflush(stderr);
}

// ============================================================
// Property access (generic)
// ============================================================

JSValue js_get_prop(JSValue obj, JSValue key) {
    if (js_is_object(obj)) {
        char* ks = js_to_cstring(key);
        JSObject* o = (JSObject*)js_as_ptr(obj);
        JSValue r = js_object_get(o, ks);
        if (js_is_undefined(r)) {
            // Check getters
            JSValue getters = js_object_get(o, "__getters");
            if (js_is_object(getters)) {
                JSValue getter_fn = js_object_get((JSObject*)js_as_ptr(getters), ks);
                if (js_is_function(getter_fn)) {
                    JSFunction* fn = (JSFunction*)js_as_ptr(getter_fn);
                    js_this_push(obj);
                    r = fn->fn(NULL, 0, fn->closure_env);
                    js_this_pop();
                }
            }
        }
        free(ks);
        return r;
    }
    if (js_is_array(obj)) {
        if (js_is_number(key)) {
            double n = js_as_number(key);
            if (n >= 0 && n == floor(n))
                return js_array_get_index(obj, (int)n);
        }
        char* ks = js_to_cstring(key);
        if (strcmp(ks, "length") == 0) {
            JSArray* arr = (JSArray*)js_as_ptr(obj);
            free(ks);
            return js_number((double)arr->length);
        }
        free(ks);
        return JS_UNDEFINED;
    }
    if (js_is_string(obj)) {
        char* ks = js_to_cstring(key);
        if (strcmp(ks, "length") == 0) {
            free(ks);
            return js_number((double)js_as_string(obj)->length);
        }
        if (js_is_number(key)) {
            int idx = (int)js_as_number(key);
            JSString* s = js_as_string(obj);
            if (idx >= 0 && idx < s->length) {
                free(ks);
                return js_string_from_len(&s->data[idx], 1);
            }
        }
        free(ks);
        return JS_UNDEFINED;
    }
    return JS_UNDEFINED;
}

void js_set_prop(JSValue obj, JSValue key, JSValue value) {
    if (js_is_object(obj)) {
        char* ks = js_to_cstring(key);
        js_object_set((JSObject*)js_as_ptr(obj), ks, value);
        free(ks);
    } else if (js_is_array(obj)) {
        if (js_is_number(key)) {
            int idx = (int)js_as_number(key);
            js_array_set_index(obj, idx, value);
        }
    }
}

