// ============================================================
// Map implementation
// ============================================================

// Map stores key-value pairs preserving insertion order
// using two parallel arrays (any type as key, unlike objects)

JSValue js_map_new(void) {
    JSValue m = js_object_new();
    JSObject* obj = (JSObject*)js_as_ptr(m);
    js_object_set(obj, "__type", js_string_from_cstr("Map"));

    JSMapData* data = (JSMapData*)malloc(sizeof(JSMapData));
    data->capacity = 8;
    data->length = 0;
    data->keys = (JSValue*)calloc(data->capacity, sizeof(JSValue));
    data->values = (JSValue*)calloc(data->capacity, sizeof(JSValue));

    // Store the data pointer as a number (pointer fits in double on 64-bit via tag)
    js_object_set(obj, "__mapdata", (JSValue)(QNAN | SIGN_BIT | 0x0004000000000000ULL | ((uint64_t)data & PTR_MASK)));
    js_object_set(obj, "size", js_number(0));
    return m;
}

JSMapData* map_get_data(JSValue map_val) {
    if (!js_is_object(map_val)) return NULL;
    JSValue d = js_object_get((JSObject*)js_as_ptr(map_val), "__mapdata");
    if (js_is_undefined(d)) return NULL;
    return (JSMapData*)((uint64_t)d & PTR_MASK);
}

int map_find_key(JSMapData* data, JSValue key) {
    for (int i = 0; i < data->length; i++) {
        if (js_strict_eq(data->keys[i], key)) return i;
    }
    return -1;
}

JSValue js_map_set(JSValue map_val, JSValue key, JSValue value) {
    JSMapData* data = map_get_data(map_val);
    if (!data) return map_val;

    int idx = map_find_key(data, key);
    if (idx >= 0) {
        data->values[idx] = value;
    } else {
        if (data->length >= data->capacity) {
            data->capacity *= 2;
            data->keys = (JSValue*)realloc(data->keys, data->capacity * sizeof(JSValue));
            data->values = (JSValue*)realloc(data->values, data->capacity * sizeof(JSValue));
        }
        data->keys[data->length] = key;
        data->values[data->length] = value;
        data->length++;
        js_object_set((JSObject*)js_as_ptr(map_val), "size", js_number((double)data->length));
    }
    return map_val;
}

JSValue js_map_get(JSValue map_val, JSValue key) {
    JSMapData* data = map_get_data(map_val);
    if (!data) return JS_UNDEFINED;
    int idx = map_find_key(data, key);
    return idx >= 0 ? data->values[idx] : JS_UNDEFINED;
}

JSValue js_map_has(JSValue map_val, JSValue key) {
    JSMapData* data = map_get_data(map_val);
    if (!data) return JS_FALSE;
    return map_find_key(data, key) >= 0 ? JS_TRUE : JS_FALSE;
}

JSValue js_map_delete(JSValue map_val, JSValue key) {
    JSMapData* data = map_get_data(map_val);
    if (!data) return JS_FALSE;
    int idx = map_find_key(data, key);
    if (idx < 0) return JS_FALSE;
    // Shift remaining entries
    for (int i = idx; i < data->length - 1; i++) {
        data->keys[i] = data->keys[i + 1];
        data->values[i] = data->values[i + 1];
    }
    data->length--;
    js_object_set((JSObject*)js_as_ptr(map_val), "size", js_number((double)data->length));
    return JS_TRUE;
}

// ============================================================
// Set implementation (simple wrapper around Map)
// ============================================================

JSValue js_set_new(void) {
    JSValue s = js_object_new();
    JSObject* obj = (JSObject*)js_as_ptr(s);
    js_object_set(obj, "__type", js_string_from_cstr("Set"));

    JSMapData* data = (JSMapData*)malloc(sizeof(JSMapData));
    data->capacity = 8;
    data->length = 0;
    data->keys = (JSValue*)calloc(data->capacity, sizeof(JSValue));
    data->values = NULL; // Set doesn't need values
    js_object_set(obj, "__mapdata", (JSValue)(QNAN | SIGN_BIT | 0x0004000000000000ULL | ((uint64_t)data & PTR_MASK)));
    js_object_set(obj, "size", js_number(0));
    return s;
}

// ============================================================
// RegExp implementation using POSIX regex
// ============================================================

JSValue js_regexp_new(JSValue pattern_val, JSValue flags_val) {
    char* pattern = js_to_cstring(pattern_val);
    char* flags = js_is_string(flags_val) ? js_to_cstring(flags_val) : _strdup("");

    JSValue re = js_object_new();
    JSObject* obj = (JSObject*)js_as_ptr(re);
    js_object_set(obj, "__type", js_string_from_cstr("RegExp"));
    js_object_set(obj, "source", js_string_from_cstr(pattern));
    js_object_set(obj, "flags", js_string_from_cstr(flags));
    js_object_set(obj, "global", strchr(flags, 'g') ? JS_TRUE : JS_FALSE);
    js_object_set(obj, "ignoreCase", strchr(flags, 'i') ? JS_TRUE : JS_FALSE);
    js_object_set(obj, "lastIndex", js_number(0));

    // Compile POSIX regex
    regex_t* compiled = (regex_t*)malloc(sizeof(regex_t));
    int cflags = REG_EXTENDED;
    if (strchr(flags, 'i')) cflags |= REG_ICASE;

    int err = regcomp(compiled, pattern, cflags);
    if (err != 0) {
        free(compiled);
        compiled = NULL;
    }

    // Store compiled regex pointer
    if (compiled) {
        js_object_set(obj, "__regex", (JSValue)(QNAN | SIGN_BIT | 0x0004000000000000ULL | ((uint64_t)compiled & PTR_MASK)));
    }

    free(pattern);
    free(flags);
    return re;
}

// RegExp.test(string) -> boolean
JSValue js_regexp_test(JSValue re_val, JSValue str_val) {
    if (!js_is_object(re_val)) return JS_FALSE;
    JSObject* obj = (JSObject*)js_as_ptr(re_val);
    JSValue regex_ptr_val = js_object_get(obj, "__regex");
    if (js_is_undefined(regex_ptr_val)) return JS_FALSE;

    regex_t* compiled = (regex_t*)((uint64_t)regex_ptr_val & PTR_MASK);
    char* str = js_to_cstring(str_val);
    int result = regexec(compiled, str, 0, NULL, 0);
    free(str);
    return result == 0 ? JS_TRUE : JS_FALSE;
}

// RegExp.exec(string) -> array or null
JSValue js_regexp_exec(JSValue re_val, JSValue str_val) {
    if (!js_is_object(re_val)) return JS_NULL;
    JSObject* obj = (JSObject*)js_as_ptr(re_val);
    JSValue regex_ptr_val = js_object_get(obj, "__regex");
    if (js_is_undefined(regex_ptr_val)) return JS_NULL;

    regex_t* compiled = (regex_t*)((uint64_t)regex_ptr_val & PTR_MASK);
    char* str = js_to_cstring(str_val);

    #define MAX_MATCHES 20
    regmatch_t matches[MAX_MATCHES];
    int result = regexec(compiled, str, MAX_MATCHES, matches, 0);

    if (result != 0) {
        free(str);
        return JS_NULL;
    }

    JSValue arr = js_array_new();
    for (int i = 0; i < MAX_MATCHES && matches[i].rm_so != -1; i++) {
        int start = matches[i].rm_so;
        int end = matches[i].rm_eo;
        js_array_push_val(arr, js_string_from_len(str + start, end - start));
    }

    // Set index property
    JSObject* arr_obj = (JSObject*)js_as_ptr(arr);
    // Arrays are not objects in our system, so we can't set properties on them directly
    // But we can return the match info differently

    free(str);
    return arr;
}

// String.match(regexp) and String.replace(regexp, replacement)
// These are handled in js_methods.c
