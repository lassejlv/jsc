// ============================================================
// Spread helpers
// ============================================================

void js_array_concat_into(JSValue target, JSValue source) {
    if (!js_is_array(source)) {
        js_array_push_val(target, source);
        return;
    }
    JSArray* src = (JSArray*)js_as_ptr(source);
    for (int i = 0; i < src->length; i++) {
        js_array_push_val(target, src->data[i]);
    }
}

void js_object_spread(JSValue target, JSValue source) {
    if (!js_is_object(source)) return;
    JSObject* src = (JSObject*)js_as_ptr(source);
    JSObject* tgt = (JSObject*)js_as_ptr(target);
    for (int i = 0; i < src->capacity; i++) {
        if (src->entries[i].occupied == 1)
            js_object_set(tgt, src->entries[i].key, src->entries[i].value);
    }
}

// ============================================================
// this binding
// ============================================================

#define MAX_THIS_DEPTH 64
static JSValue js_this_stack[MAX_THIS_DEPTH];
static int js_this_sp = 0;

void js_this_push(JSValue v) {
    if (js_this_sp < MAX_THIS_DEPTH) js_this_stack[js_this_sp++] = v;
}

void js_this_pop(void) {
    if (js_this_sp > 0) js_this_sp--;
}

JSValue js_this_get(void) {
    return js_this_sp > 0 ? js_this_stack[js_this_sp - 1] : JS_UNDEFINED;
}

// ============================================================
// Bitwise operators
// ============================================================

static int32_t to_int32(JSValue v) {
    double n = js_to_number(v);
    if (isnan(n) || isinf(n) || n == 0.0) return 0;
    return (int32_t)(int64_t)n;
}

static uint32_t to_uint32(JSValue v) {
    double n = js_to_number(v);
    if (isnan(n) || isinf(n) || n == 0.0) return 0;
    return (uint32_t)(int64_t)n;
}

JSValue js_bitand(JSValue a, JSValue b) { return js_number((double)(to_int32(a) & to_int32(b))); }
JSValue js_bitor(JSValue a, JSValue b) { return js_number((double)(to_int32(a) | to_int32(b))); }
JSValue js_bitxor(JSValue a, JSValue b) { return js_number((double)(to_int32(a) ^ to_int32(b))); }
JSValue js_shl(JSValue a, JSValue b) { return js_number((double)(to_int32(a) << (to_uint32(b) & 0x1F))); }
JSValue js_shr(JSValue a, JSValue b) { return js_number((double)(to_int32(a) >> (to_uint32(b) & 0x1F))); }
JSValue js_ushr(JSValue a, JSValue b) { return js_number((double)(to_uint32(a) >> (to_uint32(b) & 0x1F))); }
JSValue js_bitnot(JSValue a) { return js_number((double)(~to_int32(a))); }

// ============================================================
// in / instanceof / delete
// ============================================================

JSValue js_in(JSValue key, JSValue obj) {
    if (!js_is_object(obj)) return JS_FALSE;
    char* ks = js_to_cstring(key);
    JSObject* o = (JSObject*)js_as_ptr(obj);
    unsigned int idx = hash_string(ks) & (o->capacity - 1);
    unsigned int start = idx;
    int found = 0;
    while (o->entries[idx].occupied) {
        if (o->entries[idx].occupied == 1 && strcmp(o->entries[idx].key, ks) == 0) { found = 1; break; }
        idx = (idx + 1) & (o->capacity - 1);
        if (idx == start) break;
    }
    free(ks);
    return found ? JS_TRUE : JS_FALSE;
}

JSValue js_instanceof(JSValue val, JSValue ctor) {
    if (!js_is_object(val)) return JS_FALSE;
    JSObject* obj = (JSObject*)js_as_ptr(val);
    JSValue obj_type = js_object_get(obj, "__type");
    if (!js_is_string(obj_type)) return JS_FALSE;

    // If ctor is a class object with __className, compare
    if (js_is_object(ctor)) {
        JSValue ctor_name = js_object_get((JSObject*)js_as_ptr(ctor), "__className");
        if (js_is_string(ctor_name)) {
            return strcmp(js_as_string(obj_type)->data, js_as_string(ctor_name)->data) == 0
                ? JS_TRUE : JS_FALSE;
        }
    }
    return JS_FALSE;
}

JSValue js_delete_prop(JSValue obj_val, JSValue key_val) {
    if (!js_is_object(obj_val)) return JS_TRUE;
    char* ks = js_to_cstring(key_val);
    JSObject* obj = (JSObject*)js_as_ptr(obj_val);
    unsigned int idx = hash_string(ks) & (obj->capacity - 1);
    unsigned int start = idx;
    while (obj->entries[idx].occupied) {
        if (obj->entries[idx].occupied == 1 && strcmp(obj->entries[idx].key, ks) == 0) {
            free(obj->entries[idx].key);
            obj->entries[idx].key = NULL;
            obj->entries[idx].value = JS_UNDEFINED;
            obj->entries[idx].occupied = 2; // tombstone
            obj->count--;
            free(ks);
            return JS_TRUE;
        }
        idx = (idx + 1) & (obj->capacity - 1);
        if (idx == start) break;
    }
    free(ks);
    return JS_TRUE;
}

// ============================================================
// Nullish check for ??
// ============================================================

int js_is_nullish(JSValue v) {
    return js_is_null(v) || js_is_undefined(v);
}

// ============================================================
// for-in helper: returns keys array for objects, index strings for arrays
// ============================================================

JSValue js_object_keys_or_indices(JSValue v) {
    if (js_is_array(v)) {
        JSArray* arr = (JSArray*)js_as_ptr(v);
        JSValue result = js_array_new();
        for (int i = 0; i < arr->length; i++) {
            char buf[32];
            snprintf(buf, sizeof(buf), "%d", i);
            js_array_push_val(result, js_string_from_cstr(buf));
        }
        return result;
    }
    return js_object_keys(v);
}

// ============================================================
// Array.sort / Array.splice
// ============================================================

static JSValue sort_compare_fn_val;
static int sort_cmp(const void* a, const void* b) {
    JSValue va = *(const JSValue*)a;
    JSValue vb = *(const JSValue*)b;
    if (js_is_function(sort_compare_fn_val)) {
        JSFunction* fn = (JSFunction*)js_as_ptr(sort_compare_fn_val);
        JSValue args[2] = { va, vb };
        JSValue r = fn->fn(args, 2, fn->closure_env);
        return (int)js_to_number(r);
    }
    // Default: string comparison
    char* sa = js_to_cstring(va);
    char* sb = js_to_cstring(vb);
    int r = strcmp(sa, sb);
    free(sa); free(sb);
    return r;
}

JSValue js_array_sort(JSValue arr_val, JSValue compare_fn) {
    if (!js_is_array(arr_val)) return arr_val;
    JSArray* arr = (JSArray*)js_as_ptr(arr_val);
    sort_compare_fn_val = compare_fn;
    qsort(arr->data, arr->length, sizeof(JSValue), sort_cmp);
    return arr_val;
}

JSValue js_array_splice(JSValue arr_val, JSValue* args, int argc) {
    if (!js_is_array(arr_val)) return js_array_new();
    JSArray* arr = (JSArray*)js_as_ptr(arr_val);
    int start = argc > 0 ? (int)js_to_number(args[0]) : 0;
    if (start < 0) start = arr->length + start;
    if (start < 0) start = 0;
    if (start > arr->length) start = arr->length;

    int delete_count = argc > 1 ? (int)js_to_number(args[1]) : arr->length - start;
    if (delete_count < 0) delete_count = 0;
    if (start + delete_count > arr->length) delete_count = arr->length - start;

    // Collect removed elements
    JSValue removed = js_array_new();
    for (int i = 0; i < delete_count; i++) {
        js_array_push_val(removed, arr->data[start + i]);
    }

    // Items to insert
    int insert_count = argc > 2 ? argc - 2 : 0;
    int new_length = arr->length - delete_count + insert_count;

    if (insert_count != delete_count) {
        // Shift elements
        if (insert_count > delete_count) {
            while (new_length > arr->capacity) {
                arr->capacity *= 2;
                arr->data = (JSValue*)realloc(arr->data, arr->capacity * sizeof(JSValue));
            }
            memmove(&arr->data[start + insert_count], &arr->data[start + delete_count],
                    (arr->length - start - delete_count) * sizeof(JSValue));
        } else {
            memmove(&arr->data[start + insert_count], &arr->data[start + delete_count],
                    (arr->length - start - delete_count) * sizeof(JSValue));
        }
    }

    for (int i = 0; i < insert_count; i++) {
        arr->data[start + i] = args[2 + i];
    }
    arr->length = new_length;
    return removed;
}

// ============================================================
// Object.entries / Object.assign / Array.from
// ============================================================

JSValue js_object_entries(JSValue v) {
    JSValue arr = js_array_new();
    if (!js_is_object(v)) return arr;
    JSObject* obj = (JSObject*)js_as_ptr(v);
    for (int i = 0; i < obj->capacity; i++) {
        if (obj->entries[i].occupied == 1) {
            JSValue pair = js_array_new();
            js_array_push_val(pair, js_string_from_cstr(obj->entries[i].key));
            js_array_push_val(pair, obj->entries[i].value);
            js_array_push_val(arr, pair);
        }
    }
    return arr;
}

JSValue js_object_assign(JSValue target, JSValue source) {
    if (!js_is_object(target)) return target;
    if (js_is_object(source)) {
        JSObject* src = (JSObject*)js_as_ptr(source);
        JSObject* tgt = (JSObject*)js_as_ptr(target);
        for (int i = 0; i < src->capacity; i++) {
            if (src->entries[i].occupied == 1)
                js_object_set(tgt, src->entries[i].key, src->entries[i].value);
        }
    }
    return target;
}

JSValue js_array_from(JSValue v) {
    JSValue arr = js_array_new();
    if (js_is_array(v)) {
        JSArray* src = (JSArray*)js_as_ptr(v);
        for (int i = 0; i < src->length; i++) js_array_push_val(arr, src->data[i]);
    } else if (js_is_string(v)) {
        JSString* s = js_as_string(v);
        for (int i = 0; i < s->length; i++) js_array_push_val(arr, js_string_from_len(&s->data[i], 1));
    }
    return arr;
}

