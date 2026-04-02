// ============================================================
// Function/closure support
// ============================================================

JSValue js_func_new(JSNativeFunc fn, void* closure_env, int arity) {
    JSFunction* f = (JSFunction*)malloc(sizeof(JSFunction));
    f->header.type = HEAP_FUNCTION;
    f->header.refcount = 1;
    f->fn = fn;
    f->closure_env = closure_env;
    f->arity = arity;
    return (JSValue)(FUNC_TAG | ((uint64_t)f & PTR_MASK));
}

JSValue js_call_func(JSValue func_val, JSValue* args, int argc) {
    if (!js_is_function(func_val)) {
        fprintf(stderr, "TypeError: not a function\n");
        exit(1);
    }
    JSFunction* fn = (JSFunction*)js_as_ptr(func_val);
    return fn->fn(args, argc, fn->closure_env);
}

// ============================================================
// Built-in functions
// ============================================================

JSValue js_prompt(JSValue message) {
    if (!js_is_undefined(message)) {
        char* s = js_to_cstring(message);
        printf("%s", s);
        free(s);
        fflush(stdout);
    }
    char buf[4096];
    if (fgets(buf, sizeof(buf), stdin)) {
        int len = (int)strlen(buf);
        if (len > 0 && buf[len-1] == '\n') buf[--len] = '\0';
        if (len > 0 && buf[len-1] == '\r') buf[--len] = '\0';
        return js_string_from_cstr(buf);
    }
    return JS_NULL;
}

JSValue js_parse_int(JSValue str, JSValue radix) {
    char* s = js_to_cstring(str);
    int base = js_is_undefined(radix) ? 10 : (int)js_to_number(radix);
    if (base == 0) base = 10;
    char* end;
    // Skip leading whitespace
    char* p = s;
    while (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r') p++;
    long r = strtol(p, &end, base);
    int ok = (end != p);
    free(s);
    return ok ? js_number((double)r) : js_number(NAN);
}

JSValue js_parse_float(JSValue str) {
    char* s = js_to_cstring(str);
    char* end;
    double r = strtod(s, &end);
    int ok = (end != s);
    free(s);
    return ok ? js_number(r) : js_number(NAN);
}

JSValue js_isnan(JSValue v) { return isnan(js_to_number(v)) ? JS_TRUE : JS_FALSE; }
JSValue js_isfinite(JSValue v) {
    double n = js_to_number(v);
    return (!isnan(n) && !isinf(n)) ? JS_TRUE : JS_FALSE;
}

// Math
JSValue js_math_floor(JSValue v) { return js_number(floor(js_to_number(v))); }
JSValue js_math_ceil(JSValue v)  { return js_number(ceil(js_to_number(v))); }
JSValue js_math_round(JSValue v) { return js_number(round(js_to_number(v))); }
JSValue js_math_sqrt(JSValue v)  { return js_number(sqrt(js_to_number(v))); }
JSValue js_math_abs(JSValue v)   { return js_number(fabs(js_to_number(v))); }
JSValue js_math_pow(JSValue b, JSValue e) { return js_number(pow(js_to_number(b), js_to_number(e))); }
JSValue js_math_log(JSValue v)   { return js_number(log(js_to_number(v))); }
JSValue js_math_log2(JSValue v)  { return js_number(log2(js_to_number(v))); }
JSValue js_math_log10(JSValue v) { return js_number(log10(js_to_number(v))); }
JSValue js_math_sin(JSValue v)   { return js_number(sin(js_to_number(v))); }
JSValue js_math_cos(JSValue v)   { return js_number(cos(js_to_number(v))); }
JSValue js_math_tan(JSValue v)   { return js_number(tan(js_to_number(v))); }
JSValue js_math_atan2(JSValue y, JSValue x) { return js_number(atan2(js_to_number(y), js_to_number(x))); }
JSValue js_math_exp(JSValue v)   { return js_number(exp(js_to_number(v))); }
JSValue js_math_trunc(JSValue v) { return js_number(trunc(js_to_number(v))); }
JSValue js_math_sign(JSValue v) {
    double n = js_to_number(v);
    if (isnan(n)) return js_number(NAN);
    if (n > 0) return js_number(1);
    if (n < 0) return js_number(-1);
    return js_number(n);
}

static int js_rand_seeded = 0;
JSValue js_math_random(void) {
    if (!js_rand_seeded) { srand((unsigned int)time(NULL)); js_rand_seeded = 1; }
    return js_number((double)rand() / (double)RAND_MAX);
}

JSValue js_math_min(JSValue* args, int argc) {
    if (argc == 0) return js_number(INFINITY);
    double r = js_to_number(args[0]);
    for (int i = 1; i < argc; i++) {
        double n = js_to_number(args[i]);
        if (isnan(n)) return js_number(NAN);
        if (n < r) r = n;
    }
    return js_number(r);
}

JSValue js_math_max(JSValue* args, int argc) {
    if (argc == 0) return js_number(-INFINITY);
    double r = js_to_number(args[0]);
    for (int i = 1; i < argc; i++) {
        double n = js_to_number(args[i]);
        if (isnan(n)) return js_number(NAN);
        if (n > r) r = n;
    }
    return js_number(r);
}

JSValue js_Number(JSValue v) { return js_to_number_val(v); }
JSValue js_String(JSValue v) { return js_to_string_val(v); }
JSValue js_Boolean(JSValue v) { return js_is_truthy(v) ? JS_TRUE : JS_FALSE; }

// Date.now()
JSValue js_date_now(void) {
#ifdef _WIN32
    FILETIME ft;
    GetSystemTimeAsFileTime(&ft);
    uint64_t t = ((uint64_t)ft.dwHighDateTime << 32) | ft.dwLowDateTime;
    t -= 116444736000000000ULL;
    t /= 10000;
    return js_number((double)t);
#else
    struct timeval tv;
    gettimeofday(&tv, NULL);
    return js_number((double)tv.tv_sec * 1000.0 + (double)tv.tv_usec / 1000.0);
#endif
}

